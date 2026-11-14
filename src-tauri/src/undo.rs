//! The undo and redo stacks of one repository session (task 11.3).
//!
//! Every operation that can be undone runs between [`begin`] and
//! [`finish`], inside the session's write turn: its record goes on top of
//! the undo stack (at most [`MAX_ENTRIES`]), and the redo stack is emptied,
//! as in any editor. An operation that stops with conflicts is kept as
//! *pending* and recorded when it is continued to the end.
//!
//! Only the top entry of each stack is offered, and only while HEAD and the
//! local branches and tags are where it expects them: a change made outside
//! GitCRAFT makes it unavailable (with the reason) without dropping it.

use std::path::{Path, PathBuf};

use gitcraft_core::ops::journal::{self, OpKind, OpRecord, Saved, Snapshot};
use gitcraft_core::ops::status::in_progress;
use gitcraft_core::ops::undo::{Direction, UndoPreview, UndoRefusal};
use gitcraft_core::{CancellationToken, Git, Result};
use serde::Serialize;

/// Entries kept per repository and stack.
pub const MAX_ENTRIES: usize = 50;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u32,
    pub record: OpRecord,
    /// No other write since it was done: the "Undo" of its notice still
    /// applies.
    pub fresh: bool,
}

/// An operation that stopped with conflicts.
#[derive(Debug, Clone)]
pub struct Pending {
    before: Begun,
    what: Described,
}

#[derive(Debug, Default)]
pub struct UndoStack {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    pending: Option<Pending>,
}

impl UndoStack {
    /// A new operation: on top of the undo stack; the redo stack empties.
    pub fn push(&mut self, id: u32, record: OpRecord) {
        self.pending = None;
        self.redo.clear();
        self.undo.push(Entry {
            id,
            record,
            fresh: true,
        });
        if self.undo.len() > MAX_ENTRIES {
            self.undo.remove(0);
        }
    }

    /// Any write ends what the notice's "Undo" can do.
    pub fn written(&mut self) {
        for entry in &mut self.undo {
            entry.fresh = false;
        }
    }

    pub fn top(&self, direction: Direction) -> Option<&Entry> {
        match direction {
            Direction::Undo => self.undo.last(),
            Direction::Redo => self.redo.last(),
        }
    }

    /// The top entry of `direction`'s stack, if it is `id`.
    pub fn top_with(&self, direction: Direction, id: u32) -> Option<&Entry> {
        self.top(direction).filter(|e| e.id == id)
    }

    /// `id` was undone (or redone): it moves to the other stack with its
    /// updated record.
    pub fn moved(&mut self, direction: Direction, id: u32, record: OpRecord) {
        let (from, to) = match direction {
            Direction::Undo => (&mut self.undo, &mut self.redo),
            Direction::Redo => (&mut self.redo, &mut self.undo),
        };
        if from.last().is_some_and(|e| e.id == id) {
            from.pop();
        }
        to.push(Entry {
            id,
            record,
            fresh: false,
        });
        if to.len() > MAX_ENTRIES {
            to.remove(0);
        }
    }

    pub fn set_pending(&mut self, pending: Option<Pending>) {
        self.pending = pending;
    }

    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    #[cfg(test)]
    pub fn len(&self, direction: Direction) -> usize {
        match direction {
            Direction::Undo => self.undo.len(),
            Direction::Redo => self.redo.len(),
        }
    }
}

/// What an operation was, said after it ran.
#[derive(Debug, Clone)]
pub struct Described {
    pub kind: OpKind,
    /// Untracked files it moved to the Trash (a reset --hard).
    pub trashed: Vec<(String, PathBuf)>,
}

impl Described {
    pub fn kind(kind: OpKind) -> Option<Self> {
        Some(Self {
            kind,
            trashed: Vec::new(),
        })
    }
}

/// The state right before an operation.
#[derive(Debug, Clone)]
pub struct Begun {
    snapshot: Snapshot,
    stash: Option<String>,
}

/// What [`finish`] found.
#[derive(Debug, Clone)]
pub enum Finished {
    Recorded(OpRecord),
    /// It stopped with conflicts.
    Pending(Pending),
    /// Nothing to undo.
    Nothing,
}

async fn stash_top(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Option<String>> {
    let out = git
        .read(repo)
        .args(["rev-parse", "-q", "--verify", "refs/stash"])
        .accept_exit_code(1)
        .output(cancel)
        .await?;
    let oid = out.stdout_lossy().trim().to_owned();
    Ok((!oid.is_empty()).then_some(oid))
}

pub async fn begin(git: &Git, repo: &Path, cancel: &CancellationToken) -> Result<Begun> {
    Ok(Begun {
        snapshot: journal::snapshot(git, repo, cancel).await?,
        stash: stash_top(git, repo, cancel).await?,
    })
}

/// After the operation: its record, `Pending` if it stopped with conflicts,
/// or `Nothing` if it changed nothing that can be undone (or `what` is
/// `None`: it was not done).
pub async fn finish(
    git: &Git,
    repo: &Path,
    before: Begun,
    what: Option<Described>,
    cancel: &CancellationToken,
) -> Result<Finished> {
    let Some(what) = what else {
        return Ok(Finished::Nothing);
    };
    if in_progress(git, repo, cancel).await?.is_some() {
        return Ok(Finished::Pending(Pending { before, what }));
    }
    record(git, repo, before, what, cancel).await
}

/// A pending operation went on: recorded if it ended, still pending if it
/// stopped again.
pub async fn finish_pending(
    git: &Git,
    repo: &Path,
    pending: Pending,
    cancel: &CancellationToken,
) -> Result<Finished> {
    if in_progress(git, repo, cancel).await?.is_some() {
        return Ok(Finished::Pending(pending));
    }
    record(git, repo, pending.before, pending.what, cancel).await
}

async fn record(
    git: &Git,
    repo: &Path,
    before: Begun,
    what: Described,
    cancel: &CancellationToken,
) -> Result<Finished> {
    // A stash entry made by the operation: the top one changed.
    let stash = stash_top(git, repo, cancel)
        .await?
        .filter(|now| before.stash.as_ref() != Some(now));
    let saved = Saved {
        stash,
        trashed: what.trashed,
    };
    Ok(
        match journal::record(git, repo, before.snapshot, what.kind, saved, cancel).await? {
            Some(record) => Finished::Recorded(record),
            None => Finished::Nothing,
        },
    )
}

/// The top entry of a stack, as the toolbar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StackTop {
    pub id: u32,
    pub kind: OpKind,
    /// Why it cannot be done now (only the quick checks: whether something
    /// changed or an operation waits; the dialog's preview says the rest).
    pub refused: Option<UndoRefusal>,
}

/// What the toolbar's Undo and Redo offer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UndoState {
    pub undo: Option<StackTop>,
    pub redo: Option<StackTop>,
}

pub async fn stack_top(
    git: &Git,
    repo: &Path,
    entry: Option<Entry>,
    direction: Direction,
    cancel: &CancellationToken,
) -> Result<Option<StackTop>> {
    let Some(entry) = entry else {
        return Ok(None);
    };
    let expected = match direction {
        Direction::Undo => &entry.record.refs_after,
        Direction::Redo => &entry.record.refs_before,
    };
    let refused = if in_progress(git, repo, cancel).await?.is_some() {
        Some(UndoRefusal::InProgress)
    } else if &journal::current_refs(git, repo, cancel).await? != expected {
        Some(UndoRefusal::Changed)
    } else {
        None
    };
    Ok(Some(StackTop {
        id: entry.id,
        kind: entry.record.kind,
        refused,
    }))
}

/// The outcome of undoing or redoing from the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UndoResult {
    Done {
        /// The stash entry with the changes that were in the way.
        saved: Option<String>,
        exact: bool,
        /// What was staged could not be staged again.
        unstaged: bool,
    },
    /// Nothing was done; the dialog shows this preview again.
    NotDone { preview: Box<UndoPreview> },
}
