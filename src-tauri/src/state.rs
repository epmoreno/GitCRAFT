//! App-wide state: open repositories (one session per tab) and settings.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use gitcraft_core::gh::{GITHUB_HOST, Gh, GhAccount, LoginEvent};
use gitcraft_core::graph::{GraphLine, load_graph};
use gitcraft_core::ops::branch::{
    self, BranchDeletion, BranchNameCheck, CheckoutOutcome, check_branch_name,
    preview_branch_deletion,
};
use gitcraft_core::ops::clone;
use gitcraft_core::ops::commit::{CommitChanges, commit_changes, commit_details, commit_file_diff};
use gitcraft_core::ops::commit_actions::{self, CommitActions};
use gitcraft_core::ops::commit_write::{AmendInfo, amend_info, compose_message, create_commit};
use gitcraft_core::ops::config::{
    self, ConfigChange, ConfigTarget, GlobalDefaults, Identity, IdentityInfo, PullStrategy,
};
use gitcraft_core::ops::conflict::{self, ConflictFile};
use gitcraft_core::ops::discard::{
    DiscardRecord, DiscardTarget, DiscardedFile, SystemTrash, Trash, discard, preview_discard,
    undo_discard,
};
use gitcraft_core::ops::in_progress::{self, ConflictSides, ContinueOutcome};
use gitcraft_core::ops::init::{self, InitCheck};
use gitcraft_core::ops::maintenance::{
    COMMIT_GRAPH_SUGGESTION_THRESHOLD, has_commit_graph, write_commit_graph,
};
use gitcraft_core::ops::merge_rebase::{self, MergeRequest, MovePreview, MoveRecord};
use gitcraft_core::ops::pick::{self, PickOutcome, PickPreview};
use gitcraft_core::ops::publish::{self, PublishPreview, PublishRequest, Published};
use gitcraft_core::ops::refs::{RepoRefs, read_refs};
use gitcraft_core::ops::remote::{
    self, ForcePushPreview, LastFetch, PullMode, PullOutcome, PushOutcome,
};
use gitcraft_core::ops::repo::{self, RepoInfo};
use gitcraft_core::ops::reset::{self as reset_ops, ResetMode, ResetPreview, ResetRecord};
use gitcraft_core::ops::resolve::{self, MarkOutcome, ResolvedFile, Side};
use gitcraft_core::ops::stage;
use gitcraft_core::ops::stash::{self, ApplyOutcome, RecoverPreview};
use gitcraft_core::ops::status::working_status;
use gitcraft_core::ops::tag::{self, RemoteTag, TagInfo, TagNameCheck, TagPushOutcome};
use gitcraft_core::ops::worktree::{
    SelectionAction, WorktreeArea, apply_selection, worktree_file_diff,
};
use gitcraft_core::parse::commit::CommitDetails;
use gitcraft_core::parse::diff::FileDiff;
use gitcraft_core::parse::stash::StashEntry;
use gitcraft_core::parse::status::WorkingStatus;
use gitcraft_core::patch::HunkSelection;
use gitcraft_core::{CancellationToken, Git, GitError};
use serde::Serialize;

use crate::error::IpcError;
use crate::settings::{GeneralSettings, RecentRepository, SavedSession, Settings, SettingsFile};
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

/// The previous session, reopened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoredSession {
    /// Tabs, in their saved order.
    pub opened: Vec<OpenedRepository>,
    pub active: Option<RepoId>,
    /// Saved tabs that could not be opened (and were forgotten).
    pub failed: Vec<FailedRestore>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FailedRestore {
    pub path: String,
    pub error: IpcError,
}

/// The pull mode in effect for a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PullModeInfo {
    pub mode: PullMode,
    /// What git's configuration says (`branch.<name>.rebase`, `pull.rebase`).
    pub configured: PullMode,
    /// `mode` was chosen in GitCRAFT for this repository.
    pub chosen: bool,
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
    /// Stash entries were added or dropped: reload the stash list.
    pub stash: bool,
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
    /// Recent discards that can be undone, by id.
    discards: Vec<(u32, DiscardRecord)>,
    /// The last reset, merge or rebase, undoable until the next write in
    /// this repository.
    undo: Option<(u32, UndoRecord)>,
    /// Serializes this repository's writes (git's index.lock allows one).
    write_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
struct Inner {
    next_id: RepoId,
    next_discard: u32,
    sessions: Vec<RepoSession>,
    settings: Settings,
    /// The clone in progress (one at a time), to cancel it.
    clone: Option<CancellationToken>,
    /// The GitHub sign-in waiting for the browser, to cancel it.
    gh_login: Option<CancellationToken>,
}

/// Everything the "Publish on GitHub" dialog shows before publishing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublishInfo {
    pub preview: PublishPreview,
    /// From the folder name, adjusted to GitHub's rules.
    pub suggested_name: String,
    /// The account it would be created in; `None`: no valid session (or gh
    /// is not installed, see `gh_installed`).
    pub account: Option<GhAccount>,
    pub gh_installed: bool,
    /// The session could not be checked (no connection): `account` is empty
    /// but that does not mean there is no session.
    pub session_unreachable: bool,
}

/// Settings → GitHub account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GhStatus {
    /// Where gh is; `None`: not installed.
    pub path: Option<String>,
    pub version: Option<String>,
    /// Accounts for github.com (empty: no session).
    pub accounts: Vec<GhAccount>,
    /// GitHub could not be reached, so `accounts` says nothing about whether
    /// the session is valid: not the same as having no session.
    pub session_unreachable: bool,
    /// `credential.https://github.com.helper` in the global git config.
    pub github_helpers: Vec<String>,
    /// `credential.helper` in the global git config (used for every host
    /// without its own), e.g. `osxkeychain`.
    pub general_helpers: Vec<String>,
    /// git already asks gh for github.com credentials.
    pub git_uses_gh: bool,
}

pub struct AppState {
    /// `None` when git is not installed; commands then fail with `gitNotFound`.
    git: Option<Git>,
    settings_file: SettingsFile,
    notify_change: ChangeNotifier,
    /// Where discarded untracked files go (the system Trash).
    trash: Arc<dyn Trash>,
    inner: Mutex<Inner>,
}

/// What "Undo" can put back.
#[derive(Debug, Clone)]
enum UndoRecord {
    Reset(ResetRecord),
    Move(MoveRecord),
}

/// The outcome of a merge or rebase, with the id to undo it once done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub outcome: gitcraft_core::ops::merge_rebase::MoveOutcome,
    pub undo_id: Option<u32>,
}

/// The outcome of a reset, with the id to undo it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResetResult {
    pub undo_id: u32,
    /// The stash entry "GitCRAFT: antes de reset a …" with a copy of the
    /// uncommitted work, if one was made.
    pub stash: Option<String>,
    /// Untracked files moved to the Trash.
    pub trashed: u32,
}

/// Discards that can still be undone, newest last (per session).
const UNDOABLE_DISCARDS: usize = 20;

/// The outcome of a discard, with the id to undo it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscardResult {
    pub undo_id: u32,
    pub files: Vec<DiscardedFile>,
    /// A stash entry "GitCRAFT: descarte de …" holds a copy.
    pub stashed: bool,
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
            trash: Arc::new(SystemTrash),
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
        self.open_session(path, true).await
    }

    /// Opens (or finds) the session for `path`; `record` moves it to the top
    /// of the recent list (not when restoring the previous session).
    async fn open_session(&self, path: &Path, record: bool) -> Result<OpenedRepository, IpcError> {
        let git = self.git.as_ref().ok_or(GitError::GitNotFound)?;
        let info = repo::open_repository(git, path, &CancellationToken::new()).await?;

        let mut inner = self.lock();
        if record {
            inner
                .settings
                .record_opened(&info.path, &info.name, unix_now());
            self.persist(&inner.settings);
        }

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
            discards: Vec::new(),
            undo: None,
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
                stash: change.stash,
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
            let mut inner = self.lock();
            let session = inner
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or(GitError::Cancelled)?;
            // Any other write ends what "Undo" of the last reset, merge or
            // rebase can do.
            session.undo = None;
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

    /// Runs a remote operation (fetch, pull, push): one write at a time,
    /// the watcher held back, and cancellable with `cancel_operation`.
    async fn remote_operation<T, Fut>(
        &self,
        id: RepoId,
        op: impl FnOnce(PathBuf, CancellationToken) -> Fut,
    ) -> Result<T, IpcError>
    where
        Fut: std::future::Future<Output = gitcraft_core::Result<T>>,
    {
        let lock = {
            let inner = self.lock();
            let session = inner
                .sessions
                .iter()
                .find(|s| s.id == id)
                .ok_or(GitError::Cancelled)?;
            Arc::clone(&session.write_lock)
        };
        let _turn = lock.lock().await;
        let (path, cancel, quiet) = self.start_operation(id)?;
        let result = op(path, cancel).await;
        drop(quiet);
        self.finish_operation(id);
        Ok(result?)
    }

    /// Fetches from `remote` (all when `None`), reporting git's progress.
    pub async fn fetch(
        &self,
        id: RepoId,
        remote_name: Option<String>,
        prune: bool,
        on_progress: impl FnMut(&str),
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            remote::fetch(
                git,
                &repo,
                remote_name.as_deref(),
                prune,
                &cancel,
                on_progress,
            )
            .await
        })
        .await
    }

    fn session_repo_path(&self, id: RepoId) -> Result<String, IpcError> {
        Ok(self
            .lock()
            .sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.info.path.clone())
            .ok_or(GitError::Cancelled)?)
    }

    /// The pull mode in effect: the one chosen in GitCRAFT for this
    /// repository, else git's configuration for the current branch.
    pub async fn pull_mode(&self, id: RepoId) -> Result<PullModeInfo, IpcError> {
        let git = self.git()?;
        let path = self.session_repo_path(id)?;
        let chosen = self.lock().settings.pull_modes.get(&path).copied();
        let cancel = CancellationToken::new();
        let repo = PathBuf::from(&path);
        let branch = remote::current_branch(git, &repo, &cancel).await.ok();
        let configured =
            remote::configured_pull_mode(git, &repo, branch.as_deref(), &cancel).await?;
        Ok(PullModeInfo {
            mode: chosen.unwrap_or(configured),
            configured,
            chosen: chosen.is_some(),
        })
    }

    /// Chooses the pull mode for this repository (`None`: follow git's
    /// configuration again).
    pub fn set_pull_mode(&self, id: RepoId, mode: Option<PullMode>) -> Result<(), IpcError> {
        let path = self.session_repo_path(id)?;
        let mut inner = self.lock();
        match mode {
            Some(mode) => inner.settings.pull_modes.insert(path, mode),
            None => inner.settings.pull_modes.remove(&path),
        };
        self.persist(&inner.settings);
        Ok(())
    }

    /// Pulls the current branch with the pull mode in effect. With
    /// `save_local`, uncommitted changes are stashed first.
    pub async fn pull(
        &self,
        id: RepoId,
        allow_conflicts: bool,
        save_local: bool,
        on_progress: impl FnMut(&str),
    ) -> Result<PullOutcome, IpcError> {
        let git = self.git()?;
        let mode = self.pull_mode(id).await?.mode;
        self.remote_operation(id, |repo, cancel| async move {
            if save_local {
                remote::stash_and_pull(git, &repo, mode, allow_conflicts, &cancel, on_progress)
                    .await
            } else {
                remote::pull(git, &repo, mode, allow_conflicts, &cancel, on_progress).await
            }
        })
        .await
    }

    /// Pushes the current branch (or publishes it to `publish_to`); forced
    /// only with a lease on `force_lease`.
    pub async fn push(
        &self,
        id: RepoId,
        publish_to: Option<String>,
        force_lease: Option<String>,
        on_progress: impl FnMut(&str),
    ) -> Result<PushOutcome, IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            remote::push(
                git,
                &repo,
                publish_to.as_deref(),
                force_lease.as_deref(),
                &cancel,
                on_progress,
            )
            .await
        })
        .await
    }

    /// Fetches, then lists what a forced push would overwrite.
    pub async fn force_push_preview(
        &self,
        id: RepoId,
        on_progress: impl FnMut(&str),
    ) -> Result<ForcePushPreview, IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            remote::force_push_preview(git, &repo, &cancel, on_progress).await
        })
        .await
    }

    /// Aborts the merge/rebase a pull left waiting on conflicts.
    pub async fn abort_in_progress(&self, id: RepoId) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            remote::abort_in_progress(git, &repo, &cancel).await
        })
        .await
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

    /// Diff of one file's uncommitted change.
    pub async fn worktree_file_diff(
        &self,
        id: RepoId,
        path: &str,
        old_path: Option<&str>,
        area: WorktreeArea,
        full: bool,
    ) -> Result<FileDiff, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        let cancel = CancellationToken::new();
        Ok(worktree_file_diff(git, &repo, path, old_path, area, full, &cancel).await?)
    }

    /// Stages or unstages selected hunks/lines of one file.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_selection(
        &self,
        id: RepoId,
        path: String,
        old_path: Option<String>,
        area: WorktreeArea,
        fingerprint: String,
        selection: Vec<HunkSelection>,
        action: SelectionAction,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            apply_selection(
                git,
                &repo,
                &path,
                old_path.as_deref(),
                area,
                &fingerprint,
                &selection,
                action,
                &cancel,
            )
            .await
        })
        .await
    }

    /// What discarding `target` would lose (shown before confirming).
    pub async fn discard_preview(
        &self,
        id: RepoId,
        target: DiscardTarget,
    ) -> Result<Vec<DiscardedFile>, IpcError> {
        let git = self.git()?;
        let path = self.session_path(id)?;
        Ok(preview_discard(git, &path, &target, &CancellationToken::new()).await?)
    }

    /// Discards `target` after backing it up; returns an id for `undo_discard`.
    pub async fn discard(
        &self,
        id: RepoId,
        target: DiscardTarget,
    ) -> Result<DiscardResult, IpcError> {
        let git = self.git()?;
        let trash = Arc::clone(&self.trash);
        let cancel = CancellationToken::new();
        let (files, record) = self
            .write(id, |repo| async move {
                discard(git, &repo, &target, trash.as_ref(), &cancel).await
            })
            .await?;
        let stashed = record.stash.is_some();
        let mut inner = self.lock();
        inner.next_discard += 1;
        let undo_id = inner.next_discard;
        let session = inner
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(GitError::Cancelled)?;
        session.discards.push((undo_id, record));
        if session.discards.len() > UNDOABLE_DISCARDS {
            session.discards.remove(0);
        }
        Ok(DiscardResult {
            undo_id,
            files,
            stashed,
        })
    }

    /// Restores what discard `undo_id` removed, byte for byte.
    pub async fn undo_discard(&self, id: RepoId, undo_id: u32) -> Result<(), IpcError> {
        let git = self.git()?;
        let record = {
            let inner = self.lock();
            inner
                .sessions
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.discards.iter().find(|(d, _)| *d == undo_id))
                .map(|(_, r)| r.clone())
                .ok_or(GitError::Cancelled)?
        };
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            undo_discard(git, &repo, &record, &cancel).await
        })
        .await?;
        if let Some(session) = self.lock().sessions.iter_mut().find(|s| s.id == id) {
            session.discards.retain(|(d, _)| *d != undo_id);
        }
        Ok(())
    }

    /// Whether `name` can be used for a new branch.
    pub async fn check_branch_name(
        &self,
        id: RepoId,
        name: &str,
    ) -> Result<BranchNameCheck, IpcError> {
        let git = self.git()?;
        let path = self.session_path(id)?;
        Ok(check_branch_name(git, &path, name, &CancellationToken::new()).await?)
    }

    /// Creates a branch at `start` and, if asked, switches to it (`None`
    /// when not switching).
    pub async fn create_branch(
        &self,
        id: RepoId,
        name: String,
        start: String,
        track: bool,
        checkout: bool,
    ) -> Result<Option<CheckoutOutcome>, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            branch::create_branch(git, &repo, &name, &start, track, &cancel).await?;
            if !checkout {
                return Ok(None);
            }
            branch::checkout_branch(git, &repo, &name, &cancel)
                .await
                .map(Some)
        })
        .await
    }

    pub async fn rename_branch(
        &self,
        id: RepoId,
        old: String,
        new: String,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            branch::rename_branch(git, &repo, &old, &new, &cancel).await
        })
        .await
    }

    /// What deleting branch `name` would leave without a branch.
    pub async fn branch_deletion_preview(
        &self,
        id: RepoId,
        name: &str,
    ) -> Result<BranchDeletion, IpcError> {
        let git = self.git()?;
        let path = self.session_path(id)?;
        Ok(preview_branch_deletion(git, &path, name, &CancellationToken::new()).await?)
    }

    /// Deletes branch `name` if it still points to `expected_tip`.
    pub async fn delete_branch(
        &self,
        id: RepoId,
        name: String,
        expected_tip: String,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            branch::delete_branch(git, &repo, &name, &expected_tip, &cancel).await
        })
        .await
    }

    /// Switches branch, never forcing.
    pub async fn checkout_branch(
        &self,
        id: RepoId,
        name: String,
    ) -> Result<CheckoutOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            branch::checkout_branch(git, &repo, &name, &cancel).await
        })
        .await
    }

    /// Stashes every uncommitted change, then switches.
    pub async fn stash_and_checkout(
        &self,
        id: RepoId,
        name: String,
        include_untracked: bool,
    ) -> Result<CheckoutOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            branch::stash_and_checkout(git, &repo, &name, include_untracked, &cancel).await
        })
        .await
    }

    /// Every stash entry, newest first.
    pub async fn stashes(&self, id: RepoId) -> Result<Vec<StashEntry>, IpcError> {
        let git = self.git()?;
        let path = self.session_path(id)?;
        Ok(stash::list_stashes(git, &path, &CancellationToken::new()).await?)
    }

    /// Saves the uncommitted changes in a new entry; returns its id.
    pub async fn create_stash(
        &self,
        id: RepoId,
        message: String,
        include_untracked: bool,
    ) -> Result<String, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            stash::create_stash(git, &repo, &message, include_untracked, &cancel).await
        })
        .await
    }

    /// Applies (or pops) entry `oid`, never forcing; with `save_local` the
    /// current changes are stashed first.
    pub async fn apply_stash(
        &self,
        id: RepoId,
        oid: String,
        pop: bool,
        allow_conflicts: bool,
        save_local: bool,
    ) -> Result<ApplyOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            if save_local {
                stash::stash_and_apply(git, &repo, &oid, pop, allow_conflicts, &cancel).await
            } else {
                stash::apply_stash(git, &repo, &oid, pop, allow_conflicts, &cancel).await
            }
        })
        .await
    }

    /// What recovering `path` from entry `oid` would overwrite.
    pub async fn recover_preview(
        &self,
        id: RepoId,
        oid: &str,
        path: &str,
    ) -> Result<RecoverPreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(stash::preview_recover(git, &repo, oid, path, &CancellationToken::new()).await?)
    }

    /// Restores `path` from entry `oid` in the working tree, if it is still
    /// what the preview showed.
    pub async fn recover_file(
        &self,
        id: RepoId,
        oid: String,
        path: String,
        current: Option<String>,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            stash::recover_file(git, &repo, &oid, &path, current.as_deref(), &cancel).await
        })
        .await
    }

    /// Deletes entry `oid` (after the user confirmed it).
    pub async fn drop_stash(&self, id: RepoId, oid: String) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            stash::drop_stash(git, &repo, &oid, &cancel).await
        })
        .await
    }

    #[cfg(test)]
    fn with_trash(mut self, trash: Arc<dyn Trash>) -> Self {
        self.trash = trash;
        self
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
    /// What `git init` would do in `path`.
    pub async fn check_init(&self, path: &Path) -> Result<InitCheck, IpcError> {
        let git = self.git()?;
        Ok(init::check_init(git, path, &CancellationToken::new()).await?)
    }

    /// Whether `name` can be a first branch (checked in `path`).
    pub async fn valid_branch_name(&self, path: &Path, name: &str) -> Result<bool, IpcError> {
        let git = self.git()?;
        Ok(init::valid_branch_name(git, path, name, &CancellationToken::new()).await?)
    }

    /// Creates a repository in `path` and opens it.
    pub async fn init_repository(
        &self,
        path: &Path,
        branch: &str,
    ) -> Result<OpenedRepository, IpcError> {
        let git = self.git()?;
        init::init_repository(git, path, branch, &CancellationToken::new()).await?;
        self.open_repository(path).await
    }

    /// Folder suggested for a new clone: the default one from Settings,
    /// else the last one used, else home (skipping folders that are gone).
    pub fn clone_parent(&self) -> String {
        let inner = self.lock();
        let candidates = [
            inner.settings.general.clone_folder.clone(),
            inner.settings.last_clone_parent.clone(),
        ];
        drop(inner);
        candidates
            .into_iter()
            .flatten()
            .find(|p| Path::new(p).is_dir())
            .or_else(|| std::env::var("HOME").ok())
            .unwrap_or_default()
    }

    /// Git's global defaults (default branch, pull strategy).
    pub async fn git_defaults(&self) -> Result<GlobalDefaults, IpcError> {
        Ok(config::global_defaults(self.git()?, &CancellationToken::new()).await?)
    }

    /// Sets (or removes) `init.defaultBranch` in git's global config.
    pub async fn set_default_branch(
        &self,
        name: Option<String>,
    ) -> Result<GlobalDefaults, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        if let Some(name) = &name
            && !init::valid_branch_name(git, &std::env::temp_dir(), name, &cancel).await?
        {
            return Err(GitError::InvalidBranchName(name.clone()).into());
        }
        let change = ConfigChange {
            key: "init.defaultBranch".into(),
            value: name,
        };
        config::apply(git, &ConfigTarget::Global, &[change], &cancel).await?;
        self.git_defaults().await
    }

    /// Sets (or removes) the default pull strategy in git's global config.
    pub async fn set_default_pull_strategy(
        &self,
        strategy: Option<PullStrategy>,
    ) -> Result<GlobalDefaults, IpcError> {
        let git = self.git()?;
        let changes = config::pull_strategy_changes(strategy);
        config::apply(
            git,
            &ConfigTarget::Global,
            &changes,
            &CancellationToken::new(),
        )
        .await?;
        self.git_defaults().await
    }

    /// The global identity and, for tab `id`, the repository's own.
    pub async fn identity(&self, id: Option<RepoId>) -> Result<IdentityInfo, IpcError> {
        let git = self.git()?;
        let repo = id.map(|id| self.session_repo_path(id)).transpose()?;
        Ok(config::identity_info(
            git,
            repo.as_deref().map(Path::new),
            &CancellationToken::new(),
        )
        .await?)
    }

    /// Writes the identity globally (`id: None`) or in tab `id`'s repository.
    pub async fn set_identity(
        &self,
        id: Option<RepoId>,
        identity: Identity,
    ) -> Result<IdentityInfo, IpcError> {
        let git = self.git()?;
        let target = match id {
            Some(id) => ConfigTarget::Repo(PathBuf::from(self.session_repo_path(id)?)),
            None => ConfigTarget::Global,
        };
        let changes = config::identity_changes(&identity);
        config::apply(git, &target, &changes, &CancellationToken::new()).await?;
        self.identity(id).await
    }

    pub fn general_settings(&self) -> GeneralSettings {
        self.lock().settings.general.clone()
    }

    pub fn set_general_settings(&self, general: GeneralSettings) {
        let mut inner = self.lock();
        inner.settings.general = general;
        self.persist(&inner.settings);
    }

    /// Clones `url` into `parent/name` and opens it. Cancel with
    /// `cancel_clone`.
    pub async fn clone_repository(
        &self,
        url: &str,
        parent: &Path,
        name: &str,
        on_progress: impl FnMut(&str),
    ) -> Result<OpenedRepository, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        {
            let mut inner = self.lock();
            if let Some(previous) = inner.clone.replace(cancel.clone()) {
                previous.cancel();
            }
        }
        let result = clone::clone_repository(git, url, parent, name, &cancel, on_progress).await;
        {
            let mut inner = self.lock();
            inner.clone = None;
            if result.is_ok() {
                inner.settings.last_clone_parent = Some(parent.to_string_lossy().into_owned());
                self.persist(&inner.settings);
            }
        }
        self.open_repository(&result?).await
    }

    /// gh, looked up each time so installing it needs no restart. Its
    /// commands go to the same history as git's.
    fn find_gh(&self) -> Option<Gh> {
        let log = self.git.as_ref().and_then(Git::command_log);
        Gh::discover().map(|gh| match log {
            Some(log) => gh.with_log(log),
            None => gh,
        })
    }

    fn gh(&self) -> Result<Gh, IpcError> {
        Ok(self.find_gh().ok_or(GitError::GhNotFound)?)
    }

    pub async fn gh_status(&self) -> Result<GhStatus, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        let global = ConfigTarget::Global;
        let github_helpers = config::get_all(
            git,
            &global,
            &format!("credential.https://{GITHUB_HOST}.helper"),
            &cancel,
        )
        .await?;
        let general_helpers = config::get_all(git, &global, "credential.helper", &cancel).await?;
        let git_uses_gh = github_helpers
            .iter()
            .any(|h| h.contains("auth git-credential"));
        let Some(gh) = self.find_gh() else {
            return Ok(GhStatus {
                path: None,
                version: None,
                accounts: Vec::new(),
                session_unreachable: false,
                github_helpers,
                general_helpers,
                git_uses_gh,
            });
        };
        let session = gh.session(&cancel).await?;
        Ok(GhStatus {
            path: Some(gh.exe().to_string_lossy().into_owned()),
            version: gh.version(&cancel).await.ok(),
            accounts: session.accounts,
            session_unreachable: session.unreachable,
            github_helpers,
            general_helpers,
            git_uses_gh,
        })
    }

    /// Signs in through the browser: the one-time code goes to `on_event`
    /// and the device page opens in the default browser. Cancel with
    /// `cancel_gh_login`.
    pub async fn gh_login(
        &self,
        mut on_event: impl FnMut(LoginEvent) + Send,
    ) -> Result<GhStatus, IpcError> {
        let gh = self.gh()?;
        let cancel = CancellationToken::new();
        if let Some(previous) = self.lock().gh_login.replace(cancel.clone()) {
            previous.cancel();
        }
        let mut opened = false;
        let result = gh
            .login(
                |event| {
                    if let LoginEvent::Code { url, .. } = &event
                        && !opened
                    {
                        opened = true;
                        open_in_browser(url);
                    }
                    on_event(event);
                },
                &cancel,
            )
            .await;
        self.lock().gh_login = None;
        result?;
        self.gh_status().await
    }

    /// What publishing tab `id` on GitHub would upload, and where.
    pub async fn publish_info(&self, id: RepoId) -> Result<PublishInfo, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        let path = PathBuf::from(self.session_repo_path(id)?);
        let preview = publish::preview(git, &path, publish::LARGE_FILE_BYTES, &cancel).await?;
        let folder = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let gh = self.find_gh();
        let session = match &gh {
            Some(gh) => Some(gh.session(&cancel).await?),
            None => None,
        };
        let session_unreachable = session.as_ref().is_some_and(|s| s.unreachable);
        let account = session
            .filter(|s| !s.unreachable)
            .and_then(|s| s.accounts.into_iter().find(|a| a.active && a.valid));
        Ok(PublishInfo {
            preview,
            suggested_name: publish::suggest_repo_name(&folder),
            account,
            gh_installed: gh.is_some(),
            session_unreachable,
        })
    }

    /// Creates the repository on GitHub and adds it as `origin` (the push
    /// comes next, as a normal push that publishes the branch).
    pub async fn publish_repository(
        &self,
        id: RepoId,
        request: PublishRequest,
    ) -> Result<Published, IpcError> {
        let git = self.git()?;
        let path = PathBuf::from(self.session_repo_path(id)?);
        let published =
            publish::publish(git, &self.gh()?, &path, &request, &CancellationToken::new()).await?;
        // The new remote shows up in the sidebar and the toolbar at once.
        (self.notify_change)(RepoChanged {
            id,
            refs: true,
            worktree: false,
            stash: false,
        });
        Ok(published)
    }

    pub fn cancel_gh_login(&self) {
        if let Some(token) = self.lock().gh_login.take() {
            token.cancel();
        }
    }

    pub async fn gh_logout(&self, login: &str) -> Result<GhStatus, IpcError> {
        self.gh()?.logout(login, &CancellationToken::new()).await?;
        self.gh_status().await
    }

    pub async fn gh_setup_git(&self) -> Result<GhStatus, IpcError> {
        self.gh()?.setup_git(&CancellationToken::new()).await?;
        self.gh_status().await
    }

    pub fn cancel_clone(&self) {
        if let Some(token) = self.lock().clone.take() {
            token.cancel();
        }
    }

    /// Remembers the open tabs (paths in order) and the active one.
    pub fn save_session(&self, tabs: Vec<String>, active: Option<String>) {
        let mut inner = self.lock();
        let session = SavedSession { tabs, active };
        if inner.settings.session != session {
            inner.settings.session = session;
            self.persist(&inner.settings);
        }
    }

    /// Reopens the tabs of the previous session. Repositories that no longer
    /// exist or are not valid are not opened: they are reported once and
    /// dropped from the saved session.
    pub async fn restore_session(&self) -> RestoredSession {
        let saved = self.lock().settings.session.clone();
        let mut opened = Vec::new();
        let mut failed = Vec::new();
        for path in &saved.tabs {
            match self.open_session(Path::new(path), false).await {
                Ok(repo) => {
                    if !opened.iter().any(|o: &OpenedRepository| o.id == repo.id) {
                        opened.push(repo);
                    }
                }
                Err(error) => failed.push(FailedRestore {
                    path: path.clone(),
                    error,
                }),
            }
        }
        let active = saved
            .active
            .as_deref()
            .and_then(|path| opened.iter().find(|o| o.info.path == path))
            .or_else(|| opened.first())
            .map(|o| o.id);
        if !failed.is_empty() {
            let tabs = opened.iter().map(|o| o.info.path.clone()).collect();
            let active = active.and_then(|id| {
                opened
                    .iter()
                    .find(|o| o.id == id)
                    .map(|o| o.info.path.clone())
            });
            self.save_session(tabs, active);
        }
        RestoredSession {
            opened,
            active,
            failed,
        }
    }

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

/// Opens GitHub's device page (a fixed https URL from gitcraft_core, never
/// user input) with the system's opener (`open`, `xdg-open`, `explorer`),
/// without a shell.
fn open_in_browser(url: &str) {
    if !url.starts_with(&format!("https://{GITHUB_HOST}/")) {
        return;
    }
    let (program, args) = gitcraft_core::platform::Os::current().open_url_command(url);
    if let Err(err) = std::process::Command::new(program).args(args).spawn() {
        eprintln!("could not open the browser: {err}");
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Actions on a commit of the graph (task 10).
impl AppState {
    pub async fn commit_actions(&self, id: RepoId, oid: &str) -> Result<CommitActions, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(commit_actions::commit_actions(git, &repo, oid, &CancellationToken::new()).await?)
    }

    pub async fn check_tag_name(&self, id: RepoId, name: &str) -> Result<TagNameCheck, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(tag::check_tag_name(git, &repo, name, &CancellationToken::new()).await?)
    }

    pub async fn tag_info(&self, id: RepoId, name: &str) -> Result<Option<TagInfo>, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(tag::tag_info(git, &repo, name, &CancellationToken::new()).await?)
    }

    pub async fn create_tag(
        &self,
        id: RepoId,
        oid: String,
        name: String,
        message: Option<String>,
    ) -> Result<TagInfo, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            tag::create_tag(git, &repo, &oid, &name, message.as_deref(), &cancel).await
        })
        .await
    }

    pub async fn delete_tag(
        &self,
        id: RepoId,
        name: String,
        expected: String,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            tag::delete_tag(git, &repo, &name, &expected, &cancel).await
        })
        .await
    }

    /// Which remotes have tag `name` (network; cancellable).
    pub async fn remote_tags(&self, id: RepoId, name: String) -> Result<Vec<RemoteTag>, IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            tag::remote_tags(git, &repo, &name, &cancel).await
        })
        .await
    }

    pub async fn push_tag(
        &self,
        id: RepoId,
        remote: String,
        name: String,
    ) -> Result<TagPushOutcome, IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            tag::push_tag(git, &repo, &remote, &name, &cancel).await
        })
        .await
    }

    pub async fn delete_remote_tag(
        &self,
        id: RepoId,
        remote: String,
        name: String,
        expected: String,
    ) -> Result<TagPushOutcome, IpcError> {
        let git = self.git()?;
        self.remote_operation(id, |repo, cancel| async move {
            tag::delete_remote_tag(git, &repo, &remote, &name, &expected, &cancel).await
        })
        .await
    }

    pub async fn reset_preview(
        &self,
        id: RepoId,
        oid: &str,
        mode: ResetMode,
    ) -> Result<ResetPreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(reset_ops::reset_preview(git, &repo, oid, mode, &CancellationToken::new()).await?)
    }

    /// Resets the current branch (see `ops::reset`); "Undo" stays possible
    /// until the next write in this repository.
    pub async fn reset(
        &self,
        id: RepoId,
        oid: String,
        mode: ResetMode,
        expected: String,
    ) -> Result<ResetResult, IpcError> {
        let git = self.git()?;
        let trash = Arc::clone(&self.trash);
        let cancel = CancellationToken::new();
        let record = self
            .write(id, |repo| async move {
                reset_ops::reset(git, &repo, &oid, mode, &expected, trash.as_ref(), &cancel).await
            })
            .await?;
        let stash = record.stash.clone();
        let trashed = u32::try_from(record.trashed.len()).unwrap_or(u32::MAX);
        let undo_id = self.remember_undo(id, UndoRecord::Reset(record))?;
        Ok(ResetResult {
            undo_id,
            stash,
            trashed,
        })
    }

    /// Keeps `record` as what "Undo" can put back, and returns its id.
    fn remember_undo(&self, id: RepoId, record: UndoRecord) -> Result<u32, IpcError> {
        let mut inner = self.lock();
        inner.next_discard += 1;
        let undo_id = inner.next_discard;
        let session = inner
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(GitError::Cancelled)?;
        session.undo = Some((undo_id, record));
        Ok(undo_id)
    }

    /// Whether the reset, merge or rebase `undo_id` can still be undone (no
    /// write since).
    pub fn undo_available(&self, id: RepoId, undo_id: u32) -> bool {
        self.lock()
            .sessions
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.undo.as_ref())
            .is_some_and(|(u, _)| *u == undo_id)
    }

    /// Puts everything back as it was before the reset, merge or rebase
    /// `undo_id`, if nothing changed since (else `UndoConflict`, nothing
    /// touched).
    pub async fn undo_operation(&self, id: RepoId, undo_id: u32) -> Result<(), IpcError> {
        let git = self.git()?;
        let record = {
            let inner = self.lock();
            inner
                .sessions
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.undo.as_ref())
                .filter(|(u, _)| *u == undo_id)
                .map(|(_, r)| r.clone())
                .ok_or_else(|| GitError::UndoConflict(String::new()))?
        };
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            match &record {
                UndoRecord::Reset(r) => reset_ops::undo_reset(git, &repo, r, &cancel).await,
                UndoRecord::Move(r) => merge_rebase::undo_move(git, &repo, r, &cancel).await,
            }
        })
        .await
    }

    /// Merges as the user accepted it (task 8.2). When it is done, "Undo"
    /// stays possible until the next write in this repository.
    pub async fn merge(&self, id: RepoId, request: MergeRequest) -> Result<MoveResult, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        let (outcome, record) = self
            .write(id, |repo| async move {
                merge_rebase::merge(git, &repo, &request, &cancel).await
            })
            .await?;
        let undo_id = match record {
            Some(record) => Some(self.remember_undo(id, UndoRecord::Move(record))?),
            None => None,
        };
        Ok(MoveResult { outcome, undo_id })
    }

    /// When `remote` was last fetched, as far as git can tell.
    pub async fn last_fetch(&self, id: RepoId, remote: &str) -> Result<LastFetch, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(remote::last_fetch(git, &repo, remote, &CancellationToken::new()).await?)
    }

    /// What merging the branch `source` into `target` would do (task 8).
    pub async fn merge_preview(
        &self,
        id: RepoId,
        source: &str,
        target: &str,
    ) -> Result<MovePreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(
            merge_rebase::merge_preview(git, &repo, source, target, &CancellationToken::new())
                .await?,
        )
    }

    /// What rebasing the branch `source` onto `target` would do (task 8).
    pub async fn rebase_preview(
        &self,
        id: RepoId,
        source: &str,
        target: &str,
    ) -> Result<MovePreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(
            merge_rebase::rebase_preview(git, &repo, source, target, &CancellationToken::new())
                .await?,
        )
    }

    pub async fn cherry_pick_preview(
        &self,
        id: RepoId,
        oid: &str,
    ) -> Result<PickPreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(pick::cherry_pick_preview(git, &repo, oid, &CancellationToken::new()).await?)
    }

    pub async fn revert_preview(&self, id: RepoId, oid: &str) -> Result<PickPreview, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(pick::revert_preview(git, &repo, oid, &CancellationToken::new()).await?)
    }

    pub async fn revert(
        &self,
        id: RepoId,
        oid: String,
        save_changes: bool,
        allow_conflicts: bool,
    ) -> Result<PickOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            pick::revert(git, &repo, &oid, save_changes, allow_conflicts, &cancel).await
        })
        .await
    }

    pub async fn cherry_pick(
        &self,
        id: RepoId,
        oid: String,
        save_changes: bool,
        allow_conflicts: bool,
    ) -> Result<PickOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            pick::cherry_pick(git, &repo, &oid, save_changes, allow_conflicts, &cancel).await
        })
        .await
    }
}

/// Resolving conflicts (task 9): reads go straight to git; writes take the
/// session's write turn like every other change.
impl AppState {
    pub async fn conflict_file(&self, id: RepoId, path: &str) -> Result<ConflictFile, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(conflict::conflict_file(git, &repo, path, &CancellationToken::new()).await?)
    }

    pub async fn conflict_sides(&self, id: RepoId) -> Result<Option<ConflictSides>, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(in_progress::conflict_sides(git, &repo, &CancellationToken::new()).await?)
    }

    pub async fn resolved_files(&self, id: RepoId) -> Result<Vec<ResolvedFile>, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(resolve::resolved_files(git, &repo, &CancellationToken::new()).await?)
    }

    pub async fn write_resolution(
        &self,
        id: RepoId,
        path: String,
        contents: String,
        expected: String,
    ) -> Result<String, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::write_resolution(git, &repo, &path, &contents, &expected, &cancel).await
        })
        .await
    }

    pub async fn choose_version(
        &self,
        id: RepoId,
        path: String,
        side: Side,
        expected: Option<String>,
    ) -> Result<Option<String>, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::choose_version(git, &repo, &path, side, expected.as_deref(), &cancel).await
        })
        .await
    }

    pub async fn resolve_by_deleting(
        &self,
        id: RepoId,
        path: String,
        expected: Option<String>,
    ) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::resolve_by_deleting(git, &repo, &path, expected.as_deref(), &cancel).await
        })
        .await
    }

    pub async fn mark_resolved(
        &self,
        id: RepoId,
        path: String,
        force: bool,
    ) -> Result<MarkOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::mark_resolved(git, &repo, &path, force, &cancel).await
        })
        .await
    }

    pub async fn restart_conflict_file(
        &self,
        id: RepoId,
        path: String,
        expected: Option<String>,
    ) -> Result<Option<String>, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::restart_file(git, &repo, &path, expected.as_deref(), &cancel).await
        })
        .await
    }

    pub async fn prepared_message(&self, id: RepoId) -> Result<Option<String>, IpcError> {
        let git = self.git()?;
        let repo = self.session_path(id)?;
        Ok(in_progress::prepared_message(git, &repo, &CancellationToken::new()).await?)
    }

    pub async fn continue_in_progress(
        &self,
        id: RepoId,
        message: Option<String>,
    ) -> Result<ContinueOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            in_progress::continue_in_progress(git, &repo, message.as_deref(), &cancel).await
        })
        .await
    }

    pub async fn skip_current(&self, id: RepoId) -> Result<ContinueOutcome, IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            in_progress::skip_current(git, &repo, &cancel).await
        })
        .await
    }

    pub async fn unmark_resolved(&self, id: RepoId, path: String) -> Result<(), IpcError> {
        let git = self.git()?;
        let cancel = CancellationToken::new();
        self.write(id, |repo| async move {
            resolve::unmark_resolved(git, &repo, &path, &cancel).await
        })
        .await
    }
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
            Some(gitcraft_core::testing::isolated_git()),
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

    #[test]
    fn the_default_clone_folder_wins_over_the_last_one_used() {
        let f = fixture();
        let chosen = tempfile::tempdir().expect("tempdir");
        let last = tempfile::tempdir().expect("tempdir");
        let path = |d: &tempfile::TempDir| d.path().to_string_lossy().into_owned();
        f.state.lock().settings.last_clone_parent = Some(path(&last));
        assert_eq!(f.state.clone_parent(), path(&last));

        let mut general = f.state.general_settings();
        general.clone_folder = Some(path(&chosen));
        f.state.set_general_settings(general.clone());
        assert_eq!(f.state.clone_parent(), path(&chosen));
        // Saved to the settings file.
        assert_eq!(SettingsFile::new(&f.settings_path).load().general, general);

        // A default folder that is gone falls back to the last one used.
        drop(chosen);
        assert_eq!(f.state.clone_parent(), path(&last));
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
    async fn stages_a_hunk_of_a_session() {
        let f = fixture();
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        repo.write_file("main.txt", "b\ne\nnew line\n");
        let diff = f
            .state
            .worktree_file_diff(id, "main.txt", None, WorktreeArea::Unstaged, false)
            .await
            .expect("diff");
        f.state
            .apply_selection(
                id,
                "main.txt".into(),
                None,
                WorktreeArea::Unstaged,
                diff.fingerprint,
                vec![HunkSelection {
                    hunk: 0,
                    lines: None,
                }],
                SelectionAction::Stage,
            )
            .await
            .expect("stage hunk");
        let status = f.state.working_status(id).await.expect("status");
        assert_eq!(status.staged.len(), 1);
        assert!(status.unstaged.is_empty());
    }

    /// Moves "trashed" files into a temporary directory.
    struct TempTrash(tempfile::TempDir, std::sync::atomic::AtomicU32);

    impl Trash for TempTrash {
        fn trash(&self, path: &Path) -> std::io::Result<PathBuf> {
            let n = self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let to = self.0.path().join(n.to_string());
            std::fs::rename(path, &to)?;
            Ok(to)
        }
    }

    async fn hard_reset(state: &AppState, id: RepoId, target: &str) -> ResetResult {
        let preview = state
            .reset_preview(id, target, ResetMode::Hard)
            .await
            .expect("preview");
        state
            .reset(id, target.to_owned(), ResetMode::Hard, preview.fingerprint)
            .await
            .expect("reset")
    }

    #[tokio::test]
    async fn a_merge_can_be_undone_until_the_next_write() {
        let f = fixture();
        let repo = TestRepo::new().await;
        repo.commit_file("f.txt", "base\n", "base").await;
        repo.create_branch("ahead").await;
        repo.checkout("ahead").await;
        repo.commit_file("f.txt", "ahead\n", "ahead").await;
        repo.checkout("main").await;
        let base = repo.head().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        let merge = |preview: MovePreview| MergeRequest {
            source: preview.source.refname,
            target: preview.target.refname,
            expected: preview.fingerprint,
            no_ff: false,
            message: None,
            save_changes: false,
            allow_conflicts: false,
        };

        let preview = f
            .state
            .merge_preview(id, "refs/heads/ahead", "refs/heads/main")
            .await
            .expect("preview");
        let result = f.state.merge(id, merge(preview)).await.expect("merge");
        let undo_id = result.undo_id.expect("undoable");
        assert!(f.state.undo_available(id, undo_id));
        f.state.undo_operation(id, undo_id).await.expect("undo");
        assert_eq!(repo.head().await, base);

        // Any other write in the repository ends it.
        let preview = f
            .state
            .merge_preview(id, "refs/heads/ahead", "refs/heads/main")
            .await
            .expect("preview");
        let result = f.state.merge(id, merge(preview)).await.expect("merge");
        let undo_id = result.undo_id.expect("undoable");
        repo.write_file("g.txt", "new\n");
        f.state
            .stage(id, Some(vec!["g.txt".into()]))
            .await
            .expect("stage");
        assert!(!f.state.undo_available(id, undo_id));
        assert!(f.state.undo_operation(id, undo_id).await.is_err());
        assert_eq!(repo.head().await, repo.rev_parse("ahead").await);
    }

    #[tokio::test]
    async fn a_reset_can_be_undone_until_the_next_write() {
        let mut f = fixture();
        f.state = f.state.with_trash(Arc::new(TempTrash(
            tempfile::tempdir().expect("tempdir"),
            Default::default(),
        )));
        let (repo, ids) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        let head = repo.head().await;
        repo.write_file("main.txt", "b\ne\nlocal edit\n");

        let result = hard_reset(&f.state, id, &ids["M"]).await;
        assert!(result.stash.is_some());
        assert!(f.state.undo_available(id, result.undo_id));
        f.state
            .undo_operation(id, result.undo_id)
            .await
            .expect("undo");
        assert_eq!(repo.head().await, head);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("main.txt")).expect("read"),
            "b\ne\nlocal edit\n"
        );
        // Once.
        assert!(f.state.undo_operation(id, result.undo_id).await.is_err());

        // Any other write in the repository ends it.
        let result = hard_reset(&f.state, id, &ids["M"]).await;
        f.state
            .stage(id, Some(vec!["main.txt".into()]))
            .await
            .expect("stage");
        assert!(!f.state.undo_available(id, result.undo_id));
        assert!(f.state.undo_operation(id, result.undo_id).await.is_err());
        assert_eq!(repo.head().await, ids["M"], "nothing was undone");
    }

    #[tokio::test]
    async fn discards_and_undoes_once() {
        let mut f = fixture();
        f.state = f.state.with_trash(Arc::new(TempTrash(
            tempfile::tempdir().expect("tempdir"),
            Default::default(),
        )));
        let (repo, _) = TestRepo::sample_history().await;
        let id = f.state.open_repository(repo.path()).await.expect("open").id;
        repo.write_file("main.txt", "b\ne\nlocal edit\n");
        repo.write_file("scratch.txt", "x");
        let target = DiscardTarget::Files {
            paths: vec!["main.txt".into(), "scratch.txt".into()],
        };
        let preview = f
            .state
            .discard_preview(id, target.clone())
            .await
            .expect("preview");
        assert_eq!(preview.len(), 2);

        let result = f.state.discard(id, target).await.expect("discard");
        assert!(result.stashed);
        assert!(f.state.working_status(id).await.expect("status").is_clean());

        f.state
            .undo_discard(id, result.undo_id)
            .await
            .expect("undo");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("main.txt")).expect("read"),
            "b\ne\nlocal edit\n"
        );
        assert!(repo.path().join("scratch.txt").exists());
        // Each discard can be undone once.
        assert!(f.state.undo_discard(id, result.undo_id).await.is_err());
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

    #[tokio::test]
    async fn restores_the_previous_session_and_forgets_invalid_repositories() {
        let f = fixture();
        let a = TestRepo::new().await;
        let b = TestRepo::new().await;
        let gone = tempfile::tempdir().expect("tempdir");
        let not_a_repo = tempfile::tempdir().expect("tempdir");
        let path = |p: &Path| p.to_string_lossy().into_owned();
        let a_path = f
            .state
            .open_repository(a.path())
            .await
            .expect("open a")
            .info
            .path;
        let b_path = f
            .state
            .open_repository(b.path())
            .await
            .expect("open b")
            .info
            .path;
        let gone_path = path(&gone.path().join("deleted"));
        f.state.save_session(
            vec![
                b_path.clone(),
                gone_path.clone(),
                a_path.clone(),
                path(not_a_repo.path()),
            ],
            Some(a_path.clone()),
        );
        let recents = f.state.recent_repositories();

        // A new start, with the same settings file.
        let state = AppState::new(
            Some(gitcraft_core::testing::isolated_git()),
            SettingsFile::new(&f.settings_path),
            Arc::new(|_| {}),
        );
        let restored = state.restore_session().await;
        let paths: Vec<_> = restored
            .opened
            .iter()
            .map(|o| o.info.path.clone())
            .collect();
        assert_eq!(paths, [b_path.clone(), a_path.clone()], "saved order");
        assert_eq!(
            restored.active,
            restored
                .opened
                .iter()
                .find(|o| o.info.path == a_path)
                .map(|o| o.id)
        );
        let failed: Vec<_> = restored
            .failed
            .iter()
            .map(|f| (f.path.clone(), f.error.kind))
            .collect();
        assert_eq!(
            failed,
            [
                (gone_path, ErrorKind::PathNotFound),
                (path(not_a_repo.path()), ErrorKind::NotARepository)
            ]
        );
        assert_eq!(
            state.recent_repositories(),
            recents,
            "restoring does not reorder recents"
        );

        // Forgotten: the next start only reopens the valid ones, silently.
        let state = AppState::new(
            Some(gitcraft_core::testing::isolated_git()),
            SettingsFile::new(&f.settings_path),
            Arc::new(|_| {}),
        );
        let restored = state.restore_session().await;
        assert_eq!(restored.opened.len(), 2);
        assert!(restored.failed.is_empty());
    }
}
