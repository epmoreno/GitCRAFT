import { create } from "zustand";
import {
  commands,
  type CommandError,
  type PickKind,
  type PickOutcome,
  type PickPreview,
} from "../../ipc";
import type { CommitAction } from "../graph/commitActions";
import { useGraphStore } from "../graph/store";
import { refreshAfterRemote } from "../remote/store";
import { useStashStore } from "../stash/store";
import { useWorkdirStore } from "../workdir/store";

/**
 * Operations on one commit of the graph (task 10): their dialogs, their
 * result, and the changes "save my changes" put in the stash, offered back
 * with "Recover my changes" once nothing is in progress.
 */

/** Cherry-pick or revert waiting for confirmation (`preview.kind`). */
export type HistoryDialog = {
  kind: "pick";
  repoId: number;
  preview: PickPreview;
};

export type HistoryNotice =
  | {
      kind: "picked";
      repoId: number;
      pick: PickKind;
      subject: string;
      branch: string;
    }
  | { kind: "error"; repoId: number; error: CommandError };

/** Changes saved in the stash before an operation: the entry's commit, the
 * operation, and the short id of the commit it was about (in its message). */
export type SavedChanges = { oid: string; pick: PickKind; short: string };

type HistoryState = {
  dialog: HistoryDialog | null;
  notice: HistoryNotice | null;
  busy: boolean;
  saved: Record<number, SavedChanges>;
  /** Shows what cherry-picking or reverting `oid` would do. */
  requestPick: (repoId: number, oid: string, kind: PickKind) => Promise<void>;
  confirmPick: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
  /** Applies the saved changes back (the entry stays in the stash), with
   * the stash's own dialogs if something is in the way. */
  recoverSaved: (repoId: number) => Promise<void>;
  /** Stops offering them (they stay in the stash). */
  forgetSaved: (repoId: number) => void;
};

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
    default:
      return;
  }
}

/** Whether `repoId` has an operation waiting (the saved changes are offered
 * only once it is over). */
export function operationWaiting(repoId: number): boolean {
  return !!useWorkdirStore.getState().statuses[repoId]?.status?.inProgress;
}
