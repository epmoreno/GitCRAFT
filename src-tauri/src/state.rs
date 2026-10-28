//! App-wide state: open repositories (one session per tab) and settings.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use gitcraft_core::graph::{GraphLine, load_graph};
use gitcraft_core::ops::commit::{CommitChanges, commit_changes, commit_details, commit_file_diff};
use gitcraft_core::ops::commit_write::{AmendInfo, amend_info, compose_message, create_commit};
use gitcraft_core::ops::maintenance::{
    COMMIT_GRAPH_SUGGESTION_THRESHOLD, has_commit_graph, write_commit_graph,
};
use gitcraft_core::ops::refs::{RepoRefs, read_refs};
use gitcraft_core::ops::repo::{self, RepoInfo};
use gitcraft_core::ops::stage;
use gitcraft_core::ops::status::working_status;
use gitcraft_core::parse::commit::CommitDetails;
use gitcraft_core::parse::diff::FileDiff;
use gitcraft_core::parse::status::WorkingStatus;
use gitcraft_core::{CancellationToken, Git, GitError};
use serde::Serialize;

use crate::error::IpcError;
use crate::settings::{RecentRepository, Settings, SettingsFile};
use crate::watcher::{RepoWatcher, Suppression};

pub type RepoId = u32;

/// Called (from a background thread) when a repository changed on disk.
pub type ChangeNotifier = Arc<dyn Fn(RepoChanged) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OpenedRepository {
    pub id: RepoId,
    pub info: RepoInfo,
}

/// Emitted when an open repository changed outside GitCRAFT.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, specta::Type, tauri_specta::Event,
)]
#[serde(rename_all = "camelCase")]
pub struct RepoChanged {
    pub id: RepoId,
    /// HEAD, branches or tags moved: reload the graph.
    pub refs: bool,
    /// Files, the index or the merge state changed: reload the status.
    pub worktree: bool,
}

/// Sent over the graph channel: pages in order, then `done`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum GraphEvent {
    Page {
        lines: Vec<GraphLine>,
    },
    Done {
        total: u32,
        /// Large repository without a commit-graph, notice not dismissed:
        /// offer "Optimize repository".
        suggest_optimize: bool,
    },
}

/// One open repository. Will also own the write queue (see
/// docs/ARCHITECTURE.md).
struct RepoSession {
    id: RepoId,
    info: RepoInfo,
    /// Cancels the graph load in progress, if any.
    graph_load: Option<CancellationToken>,
    /// Cancels the user-started operation in progress, if any.
    operation: Option<CancellationToken>,
    /// Reports external ref changes; `None` if watching failed.
    watcher: Option<RepoWatcher>,
    /// Serializes this repository's writes (git's index.lock allows one).
    write_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct Inner {
    next_id: RepoId,
    sessions: Vec<RepoSession>,
    settings: Settings,
}

pub struct AppState {
    /// `None` when git is not installed; commands then fail with `gitNotFound`.
    git: Option<Git>,
    settings_file: SettingsFile,
    notify_change: ChangeNotifier,
    inner: Mutex<Inner>,
}

impl AppState {
    pub fn new(
        git: Option<Git>,
        settings_file: SettingsFile,
        notify_change: ChangeNotifier,
    ) -> Self {
        let settings = settings_file.load();
        Self {
            git,
            settings_file,
            notify_change,
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
            operation: None,
            watcher: self.watch(id, &info),
            write_lock: Arc::default(),
        });
        Ok(OpenedRepository { id, info })
    }

    fn watch(&self, id: RepoId, info: &RepoInfo) -> Option<RepoWatcher> {
        let notify = Arc::clone(&self.notify_change);
        let dirs = [
            PathBuf::from(&info.git_dir),
            PathBuf::from(&info.common_dir),
        ];
        RepoWatcher::start(&dirs, Path::new(&info.path), move |change| {
            notify(RepoChanged {
                id,
                refs: change.refs,
                worktree: change.worktree,
            });
        })
        .inspect_err(|err| eprintln!("not watching {}: {err}", info.path))
        .ok()
    }

    /// Returns whether a session with that id existed. Cancels its work.
    pub fn close_repository(&self, id: RepoId) -> bool {
        let mut inner = self.lock();
        let Some(index) = inner.sessions.iter().position(|s| s.id == id) else {
            return false;
        };
        let session = inner.sessions.remove(index);
        for token in [session.graph_load, session.operation]
            .into_iter()
            .flatten()
        {
            token.cancel();
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

    /// Full metadata of one commit (loaded when it is selected).
    pub async fn commit_details(&self, id: RepoId, oid: &str) -> Result<CommitDetails, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let path = self.session_path(id)?;
        Ok(commit_details(git, &path, oid, &CancellationToken::new()).await?)
    }

    /// Files changed by one commit, with line counts.
    pub async fn commit_changes(&self, id: RepoId, oid: &str) -> Result<CommitChanges, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let path = self.session_path(id)?;
        Ok(commit_changes(git, &path, oid, &CancellationToken::new()).await?)
    }

    /// Diff of one file in a commit (large diffs only when `full`).
    pub async fn commit_file_diff(
        &self,
        id: RepoId,
        oid: &str,
        path: &str,
        old_path: Option<&str>,
        full: bool,
    ) -> Result<FileDiff, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let repo = self.session_path(id)?;
        let cancel = CancellationToken::new();
        Ok(commit_file_diff(git, &repo, oid, path, old_path, full, &cancel).await?)
    }

    /// Runs a write in session `id`'s repository: one at a time, with the
    /// watcher held back until it ends (it then reports the change once).
    async fn write<T, Fut>(
        &self,
        id: RepoId,
        op: impl FnOnce(PathBuf) -> Fut,
    ) -> Result<T, IpcError>
    where
        Fut: std::future::Future<Output = gitcraft_core::Result<T>>,
    {
        let (path, lock, quiet) = {
            let inner = self.lock();
            let session = inner
                .sessions
                .iter()
                .find(|s| s.id == id)
                .ok_or(GitError::Cancelled)?;
            (
                PathBuf::from(&session.info.path),
                Arc::clone(&session.write_lock),
                session.watcher.as_ref().map(RepoWatcher::suppress),
            )
        };
        let _turn = lock.lock().await;
        let result = op(path).await;
        drop(quiet);
        Ok(result?)
    }

    fn git(&self) -> Result<&Git, IpcError> {
        Ok(self.git.as_ref().ok_or(GitError::GitNotFound)?)
    }

    /// Stages files (all changes when `paths` is `None`).
    pub async fn stage(&self, id: RepoId, paths: Option<Vec<String>>) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            match paths {
                Some(paths) => stage::stage(git, &repo, &paths, &cancel).await,
                None => stage::stage_all(git, &repo, &cancel).await,
            }
        })
        .await
    }

    /// Unstages files (everything when `paths` is `None`). The working tree
    /// is not touched.
    pub async fn unstage(&self, id: RepoId, paths: Option<Vec<String>>) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            match paths {
                Some(paths) => stage::unstage(git, &repo, &paths, &cancel).await,
                None => stage::unstage_all(git, &repo, &cancel).await,
            }
        })
        .await
    }

    /// Commits the staged changes (or amends the last commit). Returns the
    /// new commit's id.
    pub async fn commit(
        &self,
        id: RepoId,
        summary: &str,
        description: &str,
        amend: bool,
    ) -> Result<String, IpcError> {
        let git = self.git()?;
        let message = compose_message(summary, description);
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            create_commit(git, &repo, &message, amend, &cancel).await
        })
        .await
    }

    /// The last commit, to prefill and confirm an amend.
    pub async fn amend_info(&self, id: RepoId) -> Result<Option<AmendInfo>, IpcError> {
        let git = self.git()?;
        let path = self.session_path(id)?;
        Ok(amend_info(git, &path, &CancellationToken::new()).await?)
    }

    /// Working directory and index status of session `id`.
    pub async fn working_status(&self, id: RepoId) -> Result<WorkingStatus, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let path = self.session_path(id)?;
        Ok(working_status(git, &path, &CancellationToken::new()).await?)
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
        let total = load_graph(git, &path, refs.head_is_detached(), &cancel, |lines| {
            send(GraphEvent::Page { lines });
        })
        .await?;

        let suggest_optimize = total > COMMIT_GRAPH_SUGGESTION_THRESHOLD
            && !self
                .lock()
                .settings
                .optimize_notice_dismissed(&path.to_string_lossy())
            && !has_commit_graph(git, &path, &cancel).await?;
        send(GraphEvent::Done {
            total: u32::try_from(total).unwrap_or(u32::MAX),
            suggest_optimize,
        });
        Ok(())
    }

    /// Writes the commit-graph of session `id` (only ever at the user's
    /// request), forwarding git's progress lines.
    pub async fn optimize_repository(
        &self,
        id: RepoId,
        on_progress: impl FnMut(&str),
    ) -> Result<(), IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let (path, cancel, quiet) = self.start_operation(id)?;
        let result = write_commit_graph(git, &path, &cancel, on_progress).await;
        drop(quiet);
        self.finish_operation(id);
        Ok(result?)
    }

    /// Cancels the user-started operation of session `id`, if any.
    pub fn cancel_operation(&self, id: RepoId) {
        let mut inner = self.lock();
        if let Some(session) = inner.sessions.iter_mut().find(|s| s.id == id)
            && let Some(token) = session.operation.take()
        {
            token.cancel();
        }
    }

    /// Never suggest optimizing session `id`'s repository again.
    pub fn dismiss_optimize_notice(&self, id: RepoId) -> Result<(), IpcError> {
        let mut inner = self.lock();
        let path = inner
            .sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.info.path.clone())
            .ok_or(GitError::Cancelled)?;
        inner.settings.dismiss_optimize_notice(&path);
        self.persist(&inner.settings);
        Ok(())
    }

    /// Registers an own operation: its token, and a guard that holds back the
    /// watcher until it ends.
    fn start_operation(
        &self,
        id: RepoId,
    ) -> Result<(PathBuf, CancellationToken, Option<Suppression>), IpcError> {
        let mut inner = self.lock();
        let session = inner
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(GitError::Cancelled)?;
        let cancel = CancellationToken::new();
        if let Some(previous) = session.operation.replace(cancel.clone()) {
            previous.cancel();
        }
        let quiet = session.watcher.as_ref().map(RepoWatcher::suppress);
        Ok((PathBuf::from(&session.info.path), cancel, quiet))
    }

    fn finish_operation(&self, id: RepoId) {
        if let Some(session) = self.lock().sessions.iter_mut().find(|s| s.id == id) {
            session.operation = None;
        }
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
        changed: Arc<Mutex<Vec<RepoId>>>,
    }

    fn fixture() -> Fixture {
        let config = tempfile::tempdir().expect("tempdir");
        let settings_path = config.path().join("settings.json");
        let changed = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&changed);
        let state = AppState::new(
            Git::discover().ok(),
            SettingsFile::new(&settings_path),
            Arc::new(move |event: RepoChanged| sink.lock().expect("lock").push(event.id)),
        );
        Fixture {
            _config: config,
            settings_path,
            state,
            changed,
        }
    }

    async fn settle() {
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }

    #[tokio::test]
    async fn external_ref_changes_are_reported_per_session() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        settle().await;
        f.changed.lock().expect("lock").clear();

        repo.commit_file("x.txt", "x", "from the terminal").await;
        settle().await;
        assert_eq!(*f.changed.lock().expect("lock"), [id]);

        // A closed session stops watching.
        f.state.close_repository(id);
        repo.commit_file("y.txt", "y", "after closing").await;
        settle().await;
        assert_eq!(*f.changed.lock().expect("lock"), [id]);
    }

    #[tokio::test]
    async fn stages_and_unstages_files_of_a_session() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        repo.write_file("a.txt", "a");
        repo.write_file("b.txt", "b");

        f.state
            .stage(id, Some(vec!["a.txt".into()]))
            .await
            .expect("stage");
        let s = f.state.working_status(id).await.expect("status");
        assert_eq!(s.staged.len(), 1);
        assert_eq!(s.untracked.len(), 1);

        f.state.stage(id, None).await.expect("stage all");
        assert_eq!(
            f.state
                .working_status(id)
                .await
                .expect("status")
                .staged
                .len(),
            2
        );

        f.state.unstage(id, None).await.expect("unstage all");
        let s = f.state.working_status(id).await.expect("status");
        assert!(s.staged.is_empty());
        assert_eq!(s.untracked.len(), 2);
    }

    #[tokio::test]
    async fn concurrent_writes_wait_for_each_other() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        for i in 0..4 {
            repo.write_file(&format!("f{i}.txt"), "x");
        }
        // Without the write queue these would race for index.lock.
        let stage = |i: usize| f.state.stage(id, Some(vec![format!("f{i}.txt")]));
        let (a, b, c, d) = tokio::join!(stage(0), stage(1), stage(2), stage(3));
        for result in [a, b, c, d] {
            result.expect("stage");
        }
        assert_eq!(
            f.state
                .working_status(id)
                .await
                .expect("status")
                .staged
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn commits_and_amends_from_a_session() {
        let f = fixture();
        let (repo, ids) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        repo.write_file("a.txt", "a");
        f.state
            .stage(id, Some(vec!["a.txt".into()]))
            .await
            .expect("stage");
        let oid = f
            .state
            .commit(id, "Add a", "Because.", false)
            .await
            .expect("commit");
        let d = f.state.commit_details(id, &oid).await.expect("details");
        assert_eq!((d.summary.as_str(), d.body.as_str()), ("Add a", "Because."));
        assert_eq!(d.parents, [ids["E"].clone()]);

        let info = f.state.amend_info(id).await.expect("info").expect("head");
        assert_eq!(info.oid, oid);
        let amended = f
            .state
            .commit(id, "Add a (amended)", "", true)
            .await
            .expect("amend");
        let d = f.state.commit_details(id, &amended).await.expect("details");
        assert_eq!(d.summary, "Add a (amended)");
        assert_eq!(d.parents, [ids["E"].clone()]);
    }

    #[tokio::test]
    async fn reads_the_working_status_of_a_session() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        assert!(f.state.working_status(id).await.expect("status").is_clean());
        repo.write_file("new.txt", "x");
        let status = f.state.working_status(id).await.expect("status");
        assert_eq!(status.untracked.len(), 1);
    }

    #[tokio::test]
    async fn optimizing_does_not_trigger_a_refresh() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        settle().await;
        f.changed.lock().expect("lock").clear();
        f.state
            .optimize_repository(id, |_| {})
            .await
            .expect("optimize");
        settle().await;
        assert!(f.changed.lock().expect("lock").is_empty());
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
        let state = AppState::new(
            None,
            SettingsFile::new(config.path().join("s.json")),
            Arc::new(|_| {}),
        );
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
        let GraphEvent::Page { lines } = &events[0] else {
            panic!("expected a page first: {events:?}");
        };
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0].oid, ids["E"]);
        assert_eq!(
            events[1],
            GraphEvent::Done {
                total: 7,
                suggest_optimize: false
            }
        );
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

    async fn done_event(state: &AppState, id: RepoId) -> GraphEvent {
        let mut last = None;
        state
            .load_graph(id, |e| last = Some(e))
            .await
            .expect("load");
        last.expect("done event")
    }

    fn suggests(event: &GraphEvent) -> bool {
        matches!(
            event,
            GraphEvent::Done {
                suggest_optimize: true,
                ..
            }
        )
    }

    #[tokio::test]
    async fn suggests_optimizing_large_repositories_until_optimized() {
        let f = fixture();
        let repo = TestRepo::synthetic(COMMIT_GRAPH_SUGGESTION_THRESHOLD + 1).await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        assert!(suggests(&done_event(&f.state, id).await));

        let mut progress = Vec::new();
        f.state
            .optimize_repository(id, |line| progress.push(line.to_owned()))
            .await
            .expect("optimize");
        assert!(!progress.is_empty());
        assert!(!suggests(&done_event(&f.state, id).await));
    }

    #[tokio::test]
    async fn the_optimize_notice_can_be_dismissed_per_repository() {
        let f = fixture();
        let repo = TestRepo::synthetic(COMMIT_GRAPH_SUGGESTION_THRESHOLD + 1).await;
        let opened = f.state.open_repository(repo.path()).await.expect("open");
        f.state.dismiss_optimize_notice(opened.id).expect("dismiss");
        assert!(!suggests(&done_event(&f.state, opened.id).await));
        let saved = SettingsFile::new(&f.settings_path).load();
        assert_eq!(saved.optimize_notice_dismissed, [opened.info.path]);
    }

    #[tokio::test]
    async fn small_repositories_are_never_flagged() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        assert!(!suggests(&done_event(&f.state, id).await));
    }

    #[tokio::test]
    async fn reads_commit_details_of_a_session() {
        let f = fixture();
        let (repo, ids) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        let d = f
            .state
            .commit_details(id, &ids["D"])
            .await
            .expect("details");
        assert_eq!(d.summary, "D");
        assert_eq!(d.parents, [ids["C"].clone()]);
        let changes = f
            .state
            .commit_changes(id, &ids["D"])
            .await
            .expect("changes");
        assert_eq!(changes.base.as_deref(), Some(ids["C"].as_str()));
        assert_eq!(changes.files.len(), 1);
        let diff = f
            .state
            .commit_file_diff(id, &ids["D"], "feature.txt", None, false)
            .await
            .expect("diff");
        assert_eq!(diff.hunks.len(), 1);
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
