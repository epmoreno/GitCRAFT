//! Watches a repository and reports changes made from outside GitCRAFT (a
//! commit in the terminal, an editor saving a file, another client…).
//!
//! - Changes are classified: `refs` (HEAD, `packed-refs`, `refs/**`: the
//!   graph must reload) and `worktree` (files, the index and merge state:
//!   the working directory status must reload). Lock files, objects and
//!   other repositories' `.git` internals are ignored.
//! - Events are debounced: a burst (one `git commit` touches several files)
//!   produces a single notification after [`DEBOUNCE`] of quiet.
//! - While GitCRAFT's own operations run ([`RepoWatcher::suppress`]), events
//!   are held back; when the last one ends they produce at most one
//!   notification, so our own work never causes refresh loops.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// What changed during a debounce window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Change {
    pub refs: bool,
    pub worktree: bool,
}

impl Change {
    fn any(self) -> bool {
        self.refs || self.worktree
    }

    fn merge(&mut self, other: Change) {
        self.refs |= other.refs;
        self.worktree |= other.worktree;
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
        let (tx, rx) = mpsc::channel::<Message>();
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
                let mut pending = Change::default();
                // Ends when the watcher (and with it every sender) is dropped.
                while let Ok(first) = rx.recv() {
                    if let Message::Changed(change) = first {
                        pending.merge(change);
                    }
                    loop {
                        match rx.recv_timeout(DEBOUNCE) {
                            Ok(Message::Changed(change)) => pending.merge(change),
                            Ok(Message::Flush) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    if pending.any() && thread_busy.load(Ordering::SeqCst) == 0 {
                        on_change(std::mem::take(&mut pending));
                    }
                }
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
            let refs = name == "HEAD" || name == "packed-refs" || relative.starts_with("refs");
            // A moved HEAD also changes what counts as staged.
            let worktree = refs || STATUS_FILES.contains(&name.as_ref());
            return Change { refs, worktree };
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
        },
        Err(_) => Change::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Instant;

    use gitcraft_core::testing::TestRepo;

    const REFS: Change = Change {
        refs: true,
        worktree: true,
    };
    const WORKTREE: Change = Change {
        refs: false,
        worktree: true,
    };
    const NOTHING: Change = Change {
        refs: false,
        worktree: false,
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
        // FSEvents may report changes from just before the watch started.
        tokio::time::sleep(Duration::from_millis(500)).await;
        changes.lock().expect("lock").clear();
        Watched {
            repo,
            changes,
            watcher,
        }
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

    #[tokio::test]
    async fn a_burst_of_changes_is_debounced() {
        let w = watched().await;
        for i in 0..3 {
            w.repo.run(&["branch", &format!("burst-{i}")]).await;
        }
        wait_for_any(&w).await;
        settle().await;
        assert_eq!(w.changes().len(), 1);
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
}
