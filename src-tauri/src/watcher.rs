//! Watches a repository's git directories and reports ref changes made from
//! outside GitCRAFT (a commit in the terminal, another client…).
//!
//! - Only HEAD, `packed-refs` and `refs/**` matter for the graph; lock files,
//!   objects and the index are ignored.
//! - Events are debounced: a burst (one `git commit` touches several files)
//!   produces a single notification after [`DEBOUNCE`] of quiet.
//! - While GitCRAFT's own operations run ([`RepoWatcher::suppress`]), events
//!   are held back; when the last one ends they produce at most one
//!   notification, so our own work never causes refresh loops.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub const DEBOUNCE: Duration = Duration::from_millis(300);

enum Message {
    Changed,
    /// An own operation ended: re-check whether a notification is owed.
    Flush,
}

pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
    busy: Arc<AtomicUsize>,
    tx: Sender<Message>,
}

impl RepoWatcher {
    /// Watches `git_dirs` (the git dir and, for worktrees, the common dir)
    /// and calls `on_change` from a background thread.
    pub fn start(
        git_dirs: &[PathBuf],
        on_change: impl Fn() + Send + 'static,
    ) -> notify::Result<Self> {
        let roots: Vec<PathBuf> = git_dirs
            .iter()
            .map(|d| d.canonicalize().unwrap_or_else(|_| d.clone()))
            .collect();
        let (tx, rx) = mpsc::channel::<Message>();
        let busy = Arc::new(AtomicUsize::new(0));

        let event_tx = tx.clone();
        let watched_roots = roots.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                if event.paths.iter().any(|p| is_ref_change(&watched_roots, p)) {
                    let _ = event_tx.send(Message::Changed);
                }
            })?;
        let mut unique = roots.clone();
        unique.dedup();
        for root in &unique {
            watcher.watch(root, RecursiveMode::Recursive)?;
        }

        let thread_busy = Arc::clone(&busy);
        std::thread::Builder::new()
            .name("gitcraft-watcher".into())
            .spawn(move || {
                let mut pending = false;
                // Ends when the watcher (and with it every sender) is dropped.
                while let Ok(first) = rx.recv() {
                    pending |= matches!(first, Message::Changed);
                    loop {
                        match rx.recv_timeout(DEBOUNCE) {
                            Ok(Message::Changed) => pending = true,
                            Ok(Message::Flush) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    if pending && thread_busy.load(Ordering::SeqCst) == 0 {
                        pending = false;
                        on_change();
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

/// Whether `path` is HEAD, packed-refs or a ref under one of the git dirs.
fn is_ref_change(roots: &[PathBuf], path: &Path) -> bool {
    roots.iter().any(|root| {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let name = relative.to_string_lossy();
        if name.ends_with(".lock") {
            return false;
        }
        name == "HEAD" || name == "packed-refs" || relative.starts_with("refs")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    use gitcraft_core::testing::TestRepo;

    #[test]
    fn recognises_ref_paths() {
        let roots = [PathBuf::from("/r/.git")];
        let yes = [
            "/r/.git/HEAD",
            "/r/.git/packed-refs",
            "/r/.git/refs/heads/main",
        ];
        let no = [
            "/r/.git/refs/heads/main.lock",
            "/r/.git/HEAD.lock",
            "/r/.git/index",
            "/r/.git/objects/ab/cdef",
            "/r/.git/objects/info/commit-graph",
            "/r/src/main.rs",
            "/r/.git/logs/HEAD",
        ];
        for p in yes {
            assert!(is_ref_change(&roots, Path::new(p)), "{p}");
        }
        for p in no {
            assert!(!is_ref_change(&roots, Path::new(p)), "{p}");
        }
    }

    struct Watched {
        repo: TestRepo,
        count: Arc<AtomicUsize>,
        watcher: RepoWatcher,
    }

    async fn watched() -> Watched {
        let (repo, _) = TestRepo::sample_history().await;
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);
        let watcher = RepoWatcher::start(&[repo.path().join(".git")], move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watch");
        // FSEvents may report changes from just before the watch started.
        tokio::time::sleep(Duration::from_millis(500)).await;
        count.store(0, Ordering::SeqCst);
        Watched {
            repo,
            count,
            watcher,
        }
    }

    async fn wait_for(count: &AtomicUsize, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while count.load(Ordering::SeqCst) < expected && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Long enough for FSEvents latency plus the debounce.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }

    #[tokio::test]
    async fn an_external_commit_is_reported_once() {
        let w = watched().await;
        w.repo.commit_file("x.txt", "x", "external").await;
        wait_for(&w.count, 1).await;
        settle().await;
        assert_eq!(w.count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_burst_of_changes_is_debounced() {
        let w = watched().await;
        for i in 0..3 {
            w.repo.run(&["branch", &format!("burst-{i}")]).await;
        }
        wait_for(&w.count, 1).await;
        settle().await;
        assert_eq!(w.count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn working_tree_and_object_changes_are_ignored() {
        let w = watched().await;
        w.repo.write_file("untracked.txt", "x");
        w.repo.run(&["commit-graph", "write", "--reachable"]).await;
        settle().await;
        assert_eq!(w.count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn own_operations_notify_once_when_they_end() {
        let w = watched().await;
        let guard = w.watcher.suppress();
        w.repo.commit_file("x.txt", "x", "own").await;
        w.repo.run(&["branch", "own-branch"]).await;
        settle().await;
        assert_eq!(w.count.load(Ordering::SeqCst), 0, "held back while busy");
        drop(guard);
        wait_for(&w.count, 1).await;
        settle().await;
        assert_eq!(w.count.load(Ordering::SeqCst), 1);
    }
}
