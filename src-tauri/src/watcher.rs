//! Watches a repository and reports changes made from outside GitCRAFT (a
//! commit in the terminal, an editor saving a file, another client…).
//!
//! - Changes are classified: `refs` (HEAD, `packed-refs`, `refs/**`: the
//!   graph must reload) and `worktree` (files, the index and merge state:
//!   the working directory status must reload) and `stash` (`refs/stash` and
//!   its reflog: the stash list must reload). Lock files, objects and
//!   other repositories' `.git` internals are ignored.
//! - Events are debounced: a burst (one `git commit` touches several files)
//!   produces a single notification after [`DEBOUNCE`] of quiet.
//! - While GitCRAFT's own operations run ([`RepoWatcher::suppress`]), events
//!   are held back; when the last one ends they produce at most one
//!   notification, so our own work never causes refresh loops.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// What changed during a debounce window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Change {
    pub refs: bool,
    pub worktree: bool,
    pub stash: bool,
}

impl Change {
    fn any(self) -> bool {
        self.refs || self.worktree || self.stash
    }

    fn merge(&mut self, other: Change) {
        self.refs |= other.refs;
        self.worktree |= other.worktree;
        self.stash |= other.stash;
    }
}

enum Message {
    Changed(Change),
    /// An own operation ended: re-check whether a notification is owed.
    Flush,
}

/// Canonical paths watched for one repository.
#[derive(Debug, Clone)]
struct Roots {
    /// The git dir and, for linked worktrees, the common dir.
    git_dirs: Vec<PathBuf>,
    worktree: PathBuf,
}

pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
    busy: Arc<AtomicUsize>,
    tx: Sender<Message>,
}

impl RepoWatcher {
    /// Watches the worktree and its git directories, calling `on_change`
    /// from a background thread.
    pub fn start(
        git_dirs: &[PathBuf],
        worktree: &Path,
        on_change: impl Fn(Change) + Send + 'static,
    ) -> notify::Result<Self> {
        let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let roots = Roots {
            git_dirs: git_dirs.iter().map(|d| canonical(d)).collect(),
            worktree: canonical(worktree),
        };
        let (tx, mut rx) = mpsc::channel::<Message>();
        let busy = Arc::new(AtomicUsize::new(0));

        let event_tx = tx.clone();
        let classify_roots = roots.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                let mut change = Change::default();
                for path in &event.paths {
                    change.merge(classify(&classify_roots, path));
                }
                if change.any() {
                    let _ = event_tx.send(Message::Changed(change));
                }
            })?;
        // The worktree watch is recursive; git dirs inside it are covered.
        watcher.watch(&roots.worktree, RecursiveMode::Recursive)?;
        for dir in &roots.git_dirs {
            if !dir.starts_with(&roots.worktree) {
                watcher.watch(dir, RecursiveMode::Recursive)?;
            }
        }

        let thread_busy = Arc::clone(&busy);
        std::thread::Builder::new()
            .name("gitcraft-watcher".into())
            .spawn(move || {
                debounce(
                    &mut rx,
                    || thread_busy.load(Ordering::SeqCst) > 0,
                    on_change,
                )
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;

        Ok(Self {
            _watcher: watcher,
            busy,
            tx,
        })
    }

    /// Holds back notifications while an own operation runs. When the last
    /// guard is dropped, changes seen meanwhile produce one notification.
    pub fn suppress(&self) -> Suppression {
        self.busy.fetch_add(1, Ordering::SeqCst);
        Suppression {
            busy: Arc::clone(&self.busy),
            tx: self.tx.clone(),
        }
    }
}

pub struct Suppression {
    busy: Arc<AtomicUsize>,
    tx: Sender<Message>,
}

impl Drop for Suppression {
    fn drop(&mut self) {
        if self.busy.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = self.tx.send(Message::Flush);
        }
    }
}

/// Where [`debounce`] waits for messages: the channel in the app, a
/// simulated clock in the tests, so the debounce logic is tested without
/// depending on how fast the machine is.
trait Inbox {
    /// Waits for the next message; `None` when every sender is gone.
    fn recv(&mut self) -> Option<Message>;
    /// Waits at most `quiet` for the next message.
    fn recv_timeout(&mut self, quiet: Duration) -> Result<Message, RecvTimeoutError>;
}

impl Inbox for Receiver<Message> {
    fn recv(&mut self) -> Option<Message> {
        Receiver::recv(self).ok()
    }

    fn recv_timeout(&mut self, quiet: Duration) -> Result<Message, RecvTimeoutError> {
        Receiver::recv_timeout(self, quiet)
    }
}

/// Merges changes until [`DEBOUNCE`] passes without any, then reports them
/// once, unless an own operation is running: then they wait for its
/// [`Message::Flush`]. Returns when the inbox is closed.
fn debounce(inbox: &mut impl Inbox, busy: impl Fn() -> bool, mut on_change: impl FnMut(Change)) {
    let mut pending = Change::default();
    while let Some(first) = inbox.recv() {
        if let Message::Changed(change) = first {
            pending.merge(change);
        }
        loop {
            match inbox.recv_timeout(DEBOUNCE) {
                Ok(Message::Changed(change)) => pending.merge(change),
                Ok(Message::Flush) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if pending.any() && !busy() {
            on_change(std::mem::take(&mut pending));
        }
    }
}

/// Files in a git dir that change what the working directory status shows.
const STATUS_FILES: [&str; 5] = [
    "index",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "REBASE_HEAD",
];

fn classify(roots: &Roots, path: &Path) -> Change {
    for dir in &roots.git_dirs {
        if let Ok(relative) = path.strip_prefix(dir) {
            let name = relative.to_string_lossy();
            if name.ends_with(".lock") {
                return Change::default();
            }
            // Dropping an entry other than the newest only rewrites the reflog.
            let stash =
                relative == Path::new("refs/stash") || relative == Path::new("logs/refs/stash");
            let refs =
                !stash && (name == "HEAD" || name == "packed-refs" || relative.starts_with("refs"));
            // A moved HEAD also changes what counts as staged; a stash push
            // or pop changes the working tree too, which reports on its own.
            let worktree = refs || STATUS_FILES.contains(&name.as_ref());
            return Change {
                refs,
                worktree,
                stash,
            };
        }
    }
    match path.strip_prefix(&roots.worktree) {
        // Internals of nested repositories (submodules, vendored clones).
        Ok(relative)
            if relative
                .components()
                .any(|c| c == Component::Normal(".git".as_ref())) =>
        {
            Change::default()
        }
        Ok(_) => Change {
            refs: false,
            worktree: true,
            stash: false,
        },
        Err(_) => Change::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::time::Instant;

    use gitcraft_core::testing::TestRepo;

    const REFS: Change = Change {
        refs: true,
        worktree: true,
        stash: false,
    };
    const WORKTREE: Change = Change {
        refs: false,
        worktree: true,
        stash: false,
    };
    const STASH: Change = Change {
        refs: false,
        worktree: false,
        stash: true,
    };
    const NOTHING: Change = Change {
        refs: false,
        worktree: false,
        stash: false,
    };

    #[test]
    fn classifies_paths() {
        let roots = Roots {
            git_dirs: vec![PathBuf::from("/r/.git")],
            worktree: PathBuf::from("/r"),
        };
        let cases = [
            ("/r/.git/HEAD", REFS),
            ("/r/.git/packed-refs", REFS),
            ("/r/.git/refs/heads/main", REFS),
            ("/r/.git/index", WORKTREE),
            ("/r/.git/MERGE_HEAD", WORKTREE),
            ("/r/src/main.rs", WORKTREE),
            ("/r", WORKTREE),
            ("/r/.git/refs/heads/main.lock", NOTHING),
            ("/r/.git/index.lock", NOTHING),
            ("/r/.git/objects/ab/cdef", NOTHING),
            ("/r/.git/objects/info/commit-graph", NOTHING),
            ("/r/.git/logs/HEAD", NOTHING),
            ("/r/.git/refs/stash", STASH),
            ("/r/.git/logs/refs/stash", STASH),
            ("/r/.git/logs/refs/heads/main", NOTHING),
            ("/r/vendor/lib/.git/HEAD", NOTHING),
            ("/elsewhere/file", NOTHING),
        ];
        for (path, expected) in cases {
            assert_eq!(classify(&roots, Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn classifies_linked_worktrees() {
        let roots = Roots {
            git_dirs: vec![
                PathBuf::from("/main/.git/worktrees/wt"),
                PathBuf::from("/main/.git"),
            ],
            worktree: PathBuf::from("/wt"),
        };
        assert_eq!(
            classify(&roots, Path::new("/main/.git/worktrees/wt/HEAD")),
            REFS
        );
        assert_eq!(classify(&roots, Path::new("/main/.git/refs/heads/x")), REFS);
        assert_eq!(classify(&roots, Path::new("/wt/file.txt")), WORKTREE);
    }

    struct Watched {
        repo: TestRepo,
        changes: Arc<Mutex<Vec<Change>>>,
        watcher: RepoWatcher,
    }

    impl Watched {
        fn changes(&self) -> Vec<Change> {
            self.changes.lock().expect("lock").clone()
        }
    }

    async fn watched() -> Watched {
        let (repo, _) = TestRepo::sample_history().await;
        let changes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&changes);
        let watcher = RepoWatcher::start(&[repo.path().join(".git")], repo.path(), move |c| {
            sink.lock().expect("lock").push(c);
        })
        .expect("watch");
        let w = Watched {
            repo,
            changes,
            watcher,
        };
        drain_earlier_events(&w).await;
        w
    }

    /// FSEvents may deliver events from creating the repository, just before
    /// the watch started, late and at any moment (seen on CI more than half
    /// a second later). Instead of guessing a delay, write a marker no
    /// test produces (the stash reflog: the sample repository has no stash)
    /// and wait for it: events are delivered in order, so once the marker
    /// is reported every earlier event has been reported too.
    async fn drain_earlier_events(w: &Watched) {
        // Other content each time: the same one might not be an event.
        let marker = format!("{:?}", Instant::now());
        w.repo.write_file(".git/logs/refs/stash", &marker);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !w.changes().iter().any(|c| c.stash) {
            assert!(
                Instant::now() < deadline,
                "the watcher never reported the marker: {:?}",
                w.changes()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        w.changes.lock().expect("lock").clear();
    }

    async fn wait_for_any(w: &Watched) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while w.changes().is_empty() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Long enough for FSEvents latency plus the debounce.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }

    #[tokio::test]
    async fn an_external_commit_is_reported_once_as_a_ref_change() {
        let w = watched().await;
        w.repo.commit_file("x.txt", "x", "external").await;
        wait_for_any(&w).await;
        settle().await;
        assert_eq!(w.changes(), [REFS]);
    }

    /// End to end: writing a branch (one event or several, depending on the
    /// system) is reported once. Whether close changes are merged is
    /// decided by [`debounce`], tested on a simulated clock below.
    #[tokio::test]
    async fn a_branch_written_directly_is_reported_once() {
        let w = watched().await;
        let head = w.repo.head().await;
        w.repo
            .write_file(".git/refs/heads/written", &format!("{head}\n"));
        wait_for_any(&w).await;
        settle().await;
        assert_eq!(w.changes(), [REFS]);
    }

    #[tokio::test]
    async fn editing_a_file_is_a_worktree_change() {
        let w = watched().await;
        w.repo.write_file("README.md", "edited\n");
        wait_for_any(&w).await;
        settle().await;
        assert_eq!(w.changes(), [WORKTREE]);
    }

    #[tokio::test]
    async fn object_writes_are_ignored() {
        let w = watched().await;
        w.repo.run(&["commit-graph", "write", "--reachable"]).await;
        settle().await;
        assert!(w.changes().is_empty(), "{:?}", w.changes());
    }

    #[tokio::test]
    async fn own_operations_notify_once_when_they_end() {
        let w = watched().await;
        let guard = w.watcher.suppress();
        w.repo.commit_file("x.txt", "x", "own").await;
        w.repo.run(&["branch", "own-branch"]).await;
        settle().await;
        assert!(w.changes().is_empty(), "held back while busy");
        drop(guard);
        wait_for_any(&w).await;
        settle().await;
        assert_eq!(w.changes(), [REFS]);
    }

    /// What happens at a moment of the simulated clock.
    #[derive(Clone, Copy)]
    enum Step {
        Edit(Change),
        /// An own operation starts.
        Busy,
        /// The last own operation ends (as when its guard is dropped).
        Idle,
    }

    /// An inbox on a simulated clock: each step happens at a fixed
    /// millisecond, and waiting longer than the next step means a timeout.
    struct Script {
        steps: VecDeque<(Duration, Step)>,
        now: Duration,
        busy: Rc<Cell<bool>>,
    }

    impl Script {
        /// The next message no later than `limit`, applying busy changes on
        /// the way (they are not messages).
        fn next_until(&mut self, limit: Option<Duration>) -> Option<Message> {
            while let Some(&(at, step)) = self.steps.front() {
                if limit.is_some_and(|limit| at > limit) {
                    return None;
                }
                self.steps.pop_front();
                self.now = at;
                match step {
                    Step::Edit(change) => return Some(Message::Changed(change)),
                    Step::Busy => self.busy.set(true),
                    Step::Idle => {
                        self.busy.set(false);
                        return Some(Message::Flush);
                    }
                }
            }
            None
        }
    }

    impl Inbox for Script {
        fn recv(&mut self) -> Option<Message> {
            self.next_until(None)
        }

        fn recv_timeout(&mut self, quiet: Duration) -> Result<Message, RecvTimeoutError> {
            let limit = self.now + quiet;
            self.next_until(Some(limit)).ok_or_else(|| {
                self.now = limit;
                RecvTimeoutError::Timeout
            })
        }
    }

    /// What [`debounce`] reports for steps at the given milliseconds.
    fn debounced(steps: &[(u64, Step)]) -> Vec<Change> {
        let busy = Rc::new(Cell::new(false));
        let mut script = Script {
            steps: steps
                .iter()
                .map(|&(ms, step)| (Duration::from_millis(ms), step))
                .collect(),
            now: Duration::ZERO,
            busy: Rc::clone(&busy),
        };
        let mut seen = Vec::new();
        debounce(&mut script, || busy.get(), |change| seen.push(change));
        seen
    }

    const WORKTREE_AND_STASH: Change = Change {
        refs: false,
        worktree: true,
        stash: true,
    };

    #[test]
    fn changes_closer_than_the_debounce_are_reported_once() {
        // 400 ms in all, more than the debounce, but no gap reaches it.
        let seen = debounced(&[
            (0, Step::Edit(STASH)),
            (200, Step::Edit(WORKTREE)),
            (400, Step::Edit(STASH)),
        ]);
        assert_eq!(
            seen,
            [WORKTREE_AND_STASH],
            "changes closer than the debounce must be merged into one"
        );
    }

    #[test]
    fn a_pause_longer_than_the_debounce_reports_twice() {
        let seen = debounced(&[(0, Step::Edit(WORKTREE)), (1000, Step::Edit(STASH))]);
        assert_eq!(seen, [WORKTREE, STASH]);
    }

    #[test]
    fn changes_during_own_operations_wait_until_they_end() {
        let seen = debounced(&[
            (0, Step::Busy),
            (10, Step::Edit(STASH)),
            (1000, Step::Edit(WORKTREE)),
            (2000, Step::Idle),
            (5000, Step::Edit(REFS)),
        ]);
        assert_eq!(
            seen,
            [WORKTREE_AND_STASH, REFS],
            "held back while busy, then one notification"
        );
    }

    #[test]
    fn an_own_operation_without_changes_reports_nothing() {
        assert_eq!(debounced(&[(0, Step::Busy), (100, Step::Idle)]), []);
    }
}
