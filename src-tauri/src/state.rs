//! App-wide state: open repositories (one session per tab) and settings.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use gitcraft_core::graph::{GraphEntry, load_graph};
use gitcraft_core::ops::refs::{RepoRefs, read_refs};
use gitcraft_core::ops::repo::{self, RepoInfo};
use gitcraft_core::{CancellationToken, Git, GitError};
use serde::Serialize;

use crate::error::IpcError;
use crate::settings::{RecentRepository, Settings, SettingsFile};

pub type RepoId = u32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedRepository {
    pub id: RepoId,
    pub info: RepoInfo,
}

/// Sent over the graph channel: pages in order, then `done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GraphEvent {
    Page { entries: Vec<GraphEntry> },
    Done { total: u32 },
}

/// One open repository. Will also own the write queue and watcher
/// (see docs/ARCHITECTURE.md).
#[derive(Debug)]
struct RepoSession {
    id: RepoId,
    info: RepoInfo,
    /// Cancels the graph load in progress, if any.
    graph_load: Option<CancellationToken>,
}

#[derive(Debug, Default)]
struct Inner {
    next_id: RepoId,
    sessions: Vec<RepoSession>,
    settings: Settings,
}

pub struct AppState {
    /// `None` when git is not installed; commands then fail with `gitNotFound`.
    git: Option<Git>,
    settings_file: SettingsFile,
    inner: Mutex<Inner>,
}

impl AppState {
    pub fn new(git: Option<Git>, settings_file: SettingsFile) -> Self {
        let settings = settings_file.load();
        Self {
            git,
            settings_file,
            inner: Mutex::new(Inner {
                settings,
                ..Inner::default()
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep going.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Opens the repository containing `path`. Opening one that is already
    /// open returns the existing session instead of a duplicate tab.
    pub async fn open_repository(&self, path: &Path) -> Result<OpenedRepository, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let info = repo::open_repository(git, path, &CancellationToken::new()).await?;

        let mut inner = self.lock();
        inner
            .settings
            .record_opened(&info.path, &info.name, unix_now());
        self.persist(&inner.settings);

        if let Some(session) = inner.sessions.iter().find(|s| s.info.path == info.path) {
            return Ok(OpenedRepository {
                id: session.id,
                info: session.info.clone(),
            });
        }
        inner.next_id += 1;
        let id = inner.next_id;
        inner.sessions.push(RepoSession {
            id,
            info: info.clone(),
            graph_load: None,
        });
        Ok(OpenedRepository { id, info })
    }

    /// Returns whether a session with that id existed. Cancels its work.
    pub fn close_repository(&self, id: RepoId) -> bool {
        let mut inner = self.lock();
        let Some(index) = inner.sessions.iter().position(|s| s.id == id) else {
            return false;
        };
        let session = inner.sessions.remove(index);
        if let Some(load) = session.graph_load {
            load.cancel();
        }
        true
    }

    fn session_path(&self, id: RepoId) -> Result<PathBuf, IpcError> {
        self.lock()
            .sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| PathBuf::from(&s.info.path))
            .ok_or_else(|| GitError::Cancelled.into())
    }

    /// Branches, remote branches, tags and HEAD of session `id`.
    pub async fn refs(&self, id: RepoId) -> Result<RepoRefs, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let path = self.session_path(id)?;
        Ok(read_refs(git, &path, &CancellationToken::new()).await?)
    }

    /// Streams the commit graph of session `id` to `send`. Starting a new
    /// load cancels the previous one for that session. A closed session
    /// reports `cancelled`.
    pub async fn load_graph(
        &self,
        id: RepoId,
        mut send: impl FnMut(GraphEvent),
    ) -> Result<(), IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let (path, cancel) = {
            let mut inner = self.lock();
            let session = inner
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or(GitError::Cancelled)?;
            if let Some(previous) = session.graph_load.take() {
                previous.cancel();
            }
            let cancel = CancellationToken::new();
            session.graph_load = Some(cancel.clone());
            (PathBuf::from(&session.info.path), cancel)
        };

        let refs = read_refs(git, &path, &cancel).await?;
        let total = load_graph(git, &path, refs.head_is_detached(), &cancel, |entries| {
            send(GraphEvent::Page { entries });
        })
        .await?;
        send(GraphEvent::Done {
            total: u32::try_from(total).unwrap_or(u32::MAX),
        });
        Ok(())
    }

    /// Recent repositories, most recent first, without entries whose folder
    /// has disappeared.
    pub fn recent_repositories(&self) -> Vec<RecentRepository> {
        let mut inner = self.lock();
        if inner.settings.prune_missing(Path::is_dir) {
            self.persist(&inner.settings);
        }
        inner.settings.recent_repositories.clone()
    }

    /// Saving runs under the state lock so concurrent saves cannot reorder.
    /// Failing to save only loses the recent list, so it is logged, not fatal.
    fn persist(&self, settings: &Settings) {
        if let Err(err) = self.settings_file.save(settings) {
            eprintln!("could not save settings: {err}");
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcraft_core::ErrorKind;
    use gitcraft_core::testing::TestRepo;

    struct Fixture {
        _config: tempfile::TempDir,
        settings_path: std::path::PathBuf,
        state: AppState,
    }

    fn fixture() -> Fixture {
        let config = tempfile::tempdir().expect("tempdir");
        let settings_path = config.path().join("settings.json");
        let state = AppState::new(Git::discover().ok(), SettingsFile::new(&settings_path));
        Fixture {
            _config: config,
            settings_path,
            state,
        }
    }

    #[tokio::test]
    async fn opening_records_a_recent_entry_on_disk() {
        let f = fixture();
        let repo = TestRepo::new().await;
        let opened = f.state.open_repository(repo.path()).await.expect("open");
        assert_eq!(opened.id, 1);

        let recents = f.state.recent_repositories();
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].path, opened.info.path);

        // A new process sees the same list.
        let reloaded = SettingsFile::new(&f.settings_path).load();
        assert_eq!(reloaded.recent_repositories, recents);
    }

    #[tokio::test]
    async fn reopening_returns_the_existing_session() {
        let f = fixture();
        let repo = TestRepo::new().await;
        repo.write_file("sub/file.txt", "x\n");
        let first = f.state.open_repository(repo.path()).await.expect("open");
        let again = f
            .state
            .open_repository(&repo.path().join("sub"))
            .await
            .expect("open subdir");
        assert_eq!(first, again);
    }

    #[tokio::test]
    async fn closing_frees_the_session() {
        let f = fixture();
        let repo = TestRepo::new().await;
        let first = f.state.open_repository(repo.path()).await.expect("open");
        assert!(f.state.close_repository(first.id));
        assert!(!f.state.close_repository(first.id));
        let reopened = f.state.open_repository(repo.path()).await.expect("open");
        assert_ne!(reopened.id, first.id);
    }

    #[tokio::test]
    async fn failures_are_not_recorded() {
        let f = fixture();
        let plain = tempfile::tempdir().expect("tempdir");
        let err = f
            .state
            .open_repository(plain.path())
            .await
            .expect_err("not a repo");
        assert_eq!(err.kind, ErrorKind::NotARepository);
        assert!(
            err.details
                .as_deref()
                .is_some_and(|d| d.contains("not a git repository"))
        );
        assert!(f.state.recent_repositories().is_empty());
    }

    #[tokio::test]
    async fn missing_git_is_reported() {
        let config = tempfile::tempdir().expect("tempdir");
        let state = AppState::new(None, SettingsFile::new(config.path().join("s.json")));
        let err = state
            .open_repository(config.path())
            .await
            .expect_err("no git");
        assert_eq!(err.kind, ErrorKind::GitNotFound);
    }

    #[tokio::test]
    async fn streams_the_graph_then_done() {
        let f = fixture();
        let (repo, ids) = TestRepo::sample_history().await;
        let opened = f.state.open_repository(repo.path()).await.expect("open");
        let mut events = Vec::new();
        f.state
            .load_graph(opened.id, |e| events.push(e))
            .await
            .expect("load");
        assert_eq!(events.len(), 2);
        let GraphEvent::Page { entries } = &events[0] else {
            panic!("expected a page first: {events:?}");
        };
        assert_eq!(entries.len(), 7);
        assert_eq!(entries[0].commit.oid, ids["E"]);
        assert_eq!(events[1], GraphEvent::Done { total: 7 });
    }

    #[tokio::test]
    async fn reads_the_refs_of_a_session() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let opened = f.state.open_repository(repo.path()).await.expect("open");
        let refs = f.state.refs(opened.id).await.expect("refs");
        assert_eq!(refs.refs.len(), 5, "3 branches + 2 tags");
        assert_eq!(
            f.state.refs(999).await.expect_err("closed").kind,
            ErrorKind::Cancelled
        );
    }

    #[tokio::test]
    async fn loading_a_closed_session_is_cancelled() {
        let f = fixture();
        let err = f
            .state
            .load_graph(99, |_| {})
            .await
            .expect_err("unknown session");
        assert_eq!(err.kind, ErrorKind::Cancelled);
    }

    #[tokio::test]
    async fn recents_drop_deleted_repositories() {
        let f = fixture();
        let repo = TestRepo::new().await;
        f.state.open_repository(repo.path()).await.expect("open");
        let path = repo.path().to_path_buf();
        drop(repo);
        assert!(!path.exists());
        assert!(f.state.recent_repositories().is_empty());
        let reloaded = SettingsFile::new(&f.settings_path).load();
        assert!(reloaded.recent_repositories.is_empty());
    }
}
