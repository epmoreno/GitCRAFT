import { create } from "zustand";
import {
  commands,
  type CommandError,
  type PickKind,
  type PickOutcome,
  type PickPreview,
  type ResetMode,
  type ResetPreview,
} from "../../ipc";
import type { CommitAction } from "../graph/commitActions";
import { useGraphStore } from "../graph/store";
import { refreshAfterRemote } from "../remote/store";
import { useStashStore } from "../stash/store";
import { useWorkdirStore } from "../workdir/store";
import { useTagStore } from "./tags";

/**
 * Operations on one commit of the graph (task 10): their dialogs, their
 * result, and the changes "save my changes" put in the stash, offered back
 * with "Recover my changes" once nothing is in progress.
 */

/** Cherry-pick or revert waiting for confirmation (`preview.kind`), or a
 * reset, whose preview depends on the chosen mode. */
export type HistoryDialog =
  | { kind: "pick"; repoId: number; preview: PickPreview }
  | {
      kind: "reset";
      repoId: number;
      oid: string;
      mode: ResetMode;
      preview: ResetPreview | null;
      /** The repository changed since the preview the user saw. */
      changed: boolean;
    };

export type HistoryNotice =
  | {
      kind: "picked";
      repoId: number;
      pick: PickKind;
      subject: string;
      branch: string;
    }
  | {
      kind: "reset";
      repoId: number;
      undoId: number;
      branch: string;
      short: string;
      /** Undo is no longer possible (another write, or it failed). */
      undoGone: boolean;
    }
  | { kind: "resetUndone"; repoId: number; branch: string }
  | {
      kind: "merged";
      repoId: number;
      undoId: number | null;
      source: string;
      target: string;
      fastForward: boolean;
      /** Undo is no longer possible (another write, or it failed). */
      undoGone: boolean;
    }
  | { kind: "mergeUndone"; repoId: number; target: string }
  | {
      kind: "undoRefused";
      repoId: number;
      /** What could not be undone. */
      op: "reset" | "merge";
      short: string;
      stashed: boolean;
    }
  | { kind: "error"; repoId: number; error: CommandError };

/** Changes saved in the stash before an operation: the entry's commit, the
 * operation, and the short id of the commit it was about (in its message). */
export type SavedChanges = {
  oid: string;
  pick: PickKind | "reset" | "merge";
  /** The commit's short id, or the merged branch's name. */
  short: string;
};

type HistoryState = {
  dialog: HistoryDialog | null;
  notice: HistoryNotice | null;
  busy: boolean;
  saved: Record<number, SavedChanges>;
  /** Shows what cherry-picking or reverting `oid` would do. */
  requestPick: (repoId: number, oid: string, kind: PickKind) => Promise<void>;
  confirmPick: () => Promise<void>;
  /** Opens the reset dialog for `oid` (Mixed by default, like git). */
  requestReset: (repoId: number, oid: string) => Promise<void>;
  setResetMode: (mode: ResetMode) => Promise<void>;
  confirmReset: () => Promise<void>;
  /** Undoes the last reset or merge shown in the notice. */
  undo: () => Promise<void>;
  /** Checks whether the last reset or merge can still be undone. */
  checkUndo: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
  /** Applies the saved changes back (the entry stays in the stash), with
   * the stash's own dialogs if something is in the way. */
  recoverSaved: (repoId: number) => Promise<void>;
  /** Stops offering them (they stay in the stash). */
  forgetSaved: (repoId: number) => void;
  /** Offers `saved` back once nothing is in progress (task 8's merge). */
  rememberSaved: (repoId: number, saved: SavedChanges) => void;
};

/** The notices "Undo" can act on. */
function undoable(notice: HistoryNotice | null) {
  return (notice?.kind === "reset" || notice?.kind === "merged") &&
    notice.undoId !== null &&
    !notice.undoGone
    ? notice
    : null;
}

export const initialHistoryState = {
  dialog: null,
  notice: null,
  busy: false,
  saved: {},
};

/** Selects `oid` once the graph has it. */
function selectWhenLoaded(repoId: number, oid: string) {
  void useGraphStore
    .getState()
    .refresh(repoId)
    .then(() => useGraphStore.getState().select(repoId, oid));
}

export const useHistoryStore = create<HistoryState>()((set, get) => {
  const remember = (
    repoId: number,
    oid: string | null,
    pick: PickKind,
    short: string,
  ) => {
    if (!oid) return;
    set((s) => ({ saved: { ...s.saved, [repoId]: { oid, pick, short } } }));
  };

  const settle = (
    repoId: number,
    preview: PickPreview,
    outcome: PickOutcome,
  ) => {
    remember(repoId, outcome.saved, preview.kind, preview.commit.short);
    refreshAfterRemote(repoId);
    switch (outcome.type) {
      case "done":
        set({
          notice: {
            kind: "picked",
            repoId,
            pick: preview.kind,
            subject: preview.commit.subject,
            branch: preview.branch ?? "",
          },
        });
        selectWhenLoaded(repoId, outcome.head);
        return;
      case "conflicted":
        // The banner and the conflict view (task 9) take over.
        return;
      case "notDone":
        // Something changed since the preview: show it again.
        set({
          dialog: { kind: "pick", repoId, preview: outcome.preview },
        });
        return;
    }
  };

  return {
    ...initialHistoryState,

    requestPick: async (repoId, oid, kind) => {
      set({ notice: null });
      const result =
        kind === "revert"
          ? await commands.revertPreview(repoId, oid)
          : await commands.cherryPickPreview(repoId, oid);
      if (result.status === "error") {
        set({ notice: { kind: "error", repoId, error: result.error } });
        return;
      }
      set({ dialog: { kind: "pick", repoId, preview: result.data } });
    },

    confirmPick: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "pick") return;
      const { repoId, preview } = dialog;
      set({ dialog: null, busy: true });
      const run =
        preview.kind === "revert" ? commands.revert : commands.cherryPick;
      const result = await run(
        repoId,
        preview.commit.oid,
        preview.blocked !== null,
        preview.conflicts.length > 0,
      );
      set({ busy: false });
      if (result.status === "error") {
        refreshAfterRemote(repoId);
        set({ notice: { kind: "error", repoId, error: result.error } });
        return;
      }
      settle(repoId, preview, result.data);
    },

    requestReset: async (repoId, oid) => {
      set({
        notice: null,
        dialog: {
          kind: "reset",
          repoId,
          oid,
          mode: "mixed",
          preview: null,
          changed: false,
        },
      });
      await get().setResetMode("mixed");
    },

    setResetMode: async (mode) => {
      const dialog = get().dialog;
      if (dialog?.kind !== "reset") return;
      set({ dialog: { ...dialog, mode, preview: null } });
      const result = await commands.resetPreview(
        dialog.repoId,
        dialog.oid,
        mode,
      );
      const now = get().dialog;
      if (now?.kind !== "reset" || now.mode !== mode) return;
      if (result.status === "error") {
        set({
          dialog: null,
          notice: { kind: "error", repoId: dialog.repoId, error: result.error },
        });
        return;
      }
      set({ dialog: { ...now, preview: result.data } });
    },

    confirmReset: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "reset" || !dialog.preview) return;
      const { repoId, oid, mode, preview } = dialog;
      set({ busy: true });
      const result = await commands.resetBranch(
        repoId,
        oid,
        mode,
        preview.fingerprint,
      );
      set({ busy: false });
      if (result.status === "error") {
        if (result.error.kind === "staleDiff") {
          // Something changed since the preview: show the new one.
          set({ dialog: { ...dialog, changed: true } });
          await get().setResetMode(mode);
          const now = get().dialog;
          if (now?.kind === "reset") set({ dialog: { ...now, changed: true } });
          return;
        }
        set({
          dialog: null,
          notice: { kind: "error", repoId, error: result.error },
        });
        refreshAfterRemote(repoId);
        return;
      }
      const { undoId, stash } = result.data;
      set({
        dialog: null,
        notice: {
          kind: "reset",
          repoId,
          undoId,
          branch: preview.branch ?? "",
          short: preview.target.short,
          undoGone: false,
        },
      });
      if (stash) {
        set((s) => ({
          saved: {
            ...s.saved,
            [repoId]: {
              oid: stash,
              pick: "reset",
              short: preview.target.short,
            },
          },
        }));
      }
      refreshAfterRemote(repoId);
      selectWhenLoaded(repoId, preview.target.oid);
    },

    undo: async () => {
      const notice = undoable(get().notice);
      if (!notice || notice.undoId === null) return;
      const op = notice.kind === "reset" ? "reset" : "merge";
      set({ busy: true });
      const result = await commands.undoOperation(notice.repoId, notice.undoId);
      set({ busy: false });
      refreshAfterRemote(notice.repoId);
      if (result.status === "error") {
        set({
          notice: {
            kind: "undoRefused",
            repoId: notice.repoId,
            op,
            short: notice.kind === "reset" ? notice.short : notice.source,
            stashed: !!get().saved[notice.repoId],
          },
        });
        return;
      }
      // Everything is back, the copy included: nothing to recover.
      get().forgetSaved(notice.repoId);
      set({
        notice:
          notice.kind === "reset"
            ? {
                kind: "resetUndone",
                repoId: notice.repoId,
                branch: notice.branch,
              }
            : {
                kind: "mergeUndone",
                repoId: notice.repoId,
                target: notice.target,
              },
      });
    },

    checkUndo: async () => {
      const notice = undoable(get().notice);
      if (!notice || notice.undoId === null) return;
      const result = await commands.undoAvailable(notice.repoId, notice.undoId);
      const now = get().notice;
      if (now !== notice) return;
      if (result.status === "ok" && !result.data) {
        set({ notice: { ...notice, undoGone: true } });
      }
    },

    closeDialog: () => set({ dialog: null }),
    dismissNotice: () => set({ notice: null }),

    recoverSaved: async (repoId) => {
      const saved = get().saved[repoId];
      if (!saved) return;
      const stash = useStashStore.getState();
      await stash.refresh(repoId);
      const entry = useStashStore
        .getState()
        .stashes[repoId]?.entries?.find((e) => e.oid === saved.oid);
      if (!entry) {
        // Dropped or applied elsewhere: nothing left to offer.
        get().forgetSaved(repoId);
        return;
      }
      set({ notice: null });
      await stash.apply(repoId, entry, { pop: false, saveLocal: false });
    },

    rememberSaved: (repoId, saved) =>
      set((s) => ({ saved: { ...s.saved, [repoId]: saved } })),

    forgetSaved: (repoId) =>
      set((s) => {
        const saved = { ...s.saved };
        delete saved[repoId];
        return { saved };
      }),
  };
});

// Once the stash says the saved changes were applied (even with conflicts),
// stop offering them.
useStashStore.subscribe((state, previous) => {
  const notice = state.notice;
  if (!notice || notice === previous.notice) return;
  if (notice.kind === "applied" || notice.kind === "conflicted") {
    if (useHistoryStore.getState().saved[notice.repoId]) {
      useHistoryStore.getState().forgetSaved(notice.repoId);
    }
  }
});

/** What picking `action` in a commit's menu or panel does. */
export function runCommitAction(
  action: CommitAction,
  repoId: number,
  oid: string,
): void {
  switch (action) {
    case "cherryPick":
    case "revert":
      void useHistoryStore.getState().requestPick(repoId, oid, action);
      return;
    case "reset":
      void useHistoryStore.getState().requestReset(repoId, oid);
      return;
    case "createTag":
      void useTagStore.getState().requestCreate(repoId, oid);
      return;
    default:
      return;
  }
}

/** Whether `repoId` has an operation waiting (the saved changes are offered
 * only once it is over). */
export function operationWaiting(repoId: number): boolean {
  return !!useWorkdirStore.getState().statuses[repoId]?.status?.inProgress;
}
