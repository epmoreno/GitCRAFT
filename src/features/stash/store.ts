import { create } from "zustand";
import {
  commands,
  type ApplyOutcome,
  type BlockingFile,
  type CommandError,
  type RecoverPreview,
  type StashEntry,
} from "../../ipc";
import { stashSelection, useGraphStore } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";

export type RepoStashes = {
  entries: StashEntry[] | null;
  error: CommandError | null;
  /** When the list was read (ms): relative dates are shown from then. */
  readAt: number;
};

/** How an entry is being applied. */
export type ApplyMode = {
  pop: boolean;
  /** Stash the current changes first. */
  saveLocal: boolean;
};

/** The stash dialog on screen, if any. */
export type StashDialog =
  | { kind: "create"; repoId: number }
  | {
      kind: "blocked";
      repoId: number;
      entry: StashEntry;
      mode: ApplyMode;
      files: BlockingFile[];
    }
  | {
      kind: "conflict";
      repoId: number;
      entry: StashEntry;
      mode: ApplyMode;
      files: string[];
      exact: boolean;
    }
  | { kind: "drop"; repoId: number; entry: StashEntry }
  | {
      kind: "recover";
      repoId: number;
      entry: StashEntry;
      preview: RecoverPreview;
    };

/** What the stash toast reports after an operation. */
export type StashNotice =
  | { kind: "error"; repoId: number; error: CommandError }
  | { kind: "created"; repoId: number }
  | { kind: "applied"; repoId: number; dropped: boolean; savedLocal: boolean }
  | { kind: "conflicted"; repoId: number; files: string[] }
  | { kind: "dropped"; repoId: number; entry: StashEntry }
  | { kind: "recovered"; repoId: number; path: string }
  | { kind: "alreadyRecovered"; repoId: number; path: string };

type StashState = {
  stashes: Record<number, RepoStashes>;
  dialog: StashDialog | null;
  notice: StashNotice | null;
  busy: boolean;
  refresh: (repoId: number) => Promise<void>;
  requestCreate: (repoId: number) => void;
  create: (message: string, includeUntracked: boolean) => Promise<void>;
  /** Applies (or pops) an entry; explains instead when something is in
   * the way. */
  apply: (
    repoId: number,
    entry: StashEntry,
    mode: ApplyMode,
    allowConflicts?: boolean,
  ) => Promise<void>;
  /** From the "blocked" dialog: save my changes, then apply. */
  saveLocalAndApply: () => Promise<void>;
  /** From the "conflict" dialog: apply leaving conflict markers. */
  applyWithConflicts: () => Promise<void>;
  /** "Recover file": straight away when nothing would be overwritten,
   * otherwise after showing what would. */
  requestRecover: (
    repoId: number,
    entry: StashEntry,
    path: string,
  ) => Promise<void>;
  confirmRecover: () => Promise<void>;
  requestDrop: (repoId: number, entry: StashEntry) => void;
  confirmDrop: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
  forget: (repoId: number) => void;
};

export const initialStashState = {
  stashes: {},
  dialog: null,
  notice: null,
  busy: false,
} satisfies Partial<StashState>;

/** Latest list request per repository: older answers are dropped. */
const latest = new Map<number, number>();
let nextRequest = 0;

export const useStashStore = create<StashState>()((set, get) => {
  /** After any stash operation: the list, the status and the history. */
  const afterWrite = (repoId: number) => {
    void get().refresh(repoId);
    void useWorkdirStore.getState().refresh(repoId);
    void useGraphStore.getState().refresh(repoId);
  };

  /** An entry that is gone stops being shown in the detail panel. */
  const deselect = (repoId: number, oid: string) => {
    const graph = useGraphStore.getState();
    if (graph.graphs[repoId]?.selectedOid === stashSelection(oid)) {
      graph.select(repoId, null);
    }
  };

  const fail = (repoId: number, error: CommandError) =>
    set({ notice: { kind: "error", repoId, error } });

  const recover = async (
    repoId: number,
    entry: StashEntry,
    preview: RecoverPreview,
  ) => {
    set({ busy: true });
    const result = await commands.recoverFile(
      repoId,
      entry.oid,
      preview.path,
      preview.current,
    );
    set({ busy: false });
    afterWrite(repoId);
    if (result.status === "error") fail(repoId, result.error);
    else set({ notice: { kind: "recovered", repoId, path: preview.path } });
  };

  const settle = (
    repoId: number,
    entry: StashEntry,
    mode: ApplyMode,
    outcome: ApplyOutcome,
  ) => {
    switch (outcome.type) {
      case "applied":
        if (outcome.dropped) deselect(repoId, entry.oid);
        set({
          notice: {
            kind: "applied",
            repoId,
            dropped: outcome.dropped,
            savedLocal: mode.saveLocal,
          },
        });
        break;
      case "blocked":
        set({
          dialog: {
            kind: "blocked",
            repoId,
            entry,
            mode,
            files: outcome.files,
          },
        });
        break;
      case "wouldConflict":
        set({
          dialog: {
            kind: "conflict",
            repoId,
            entry,
            mode,
            files: outcome.files,
            exact: outcome.exact,
          },
        });
        break;
      case "conflicted":
        set({ notice: { kind: "conflicted", repoId, files: outcome.files } });
        break;
    }
  };

  return {
    ...initialStashState,

    refresh: async (repoId) => {
      const request = ++nextRequest;
      latest.set(repoId, request);
      const result = await commands.listStashes(repoId);
      if (latest.get(repoId) !== request) return;
      if (result.status === "error" && result.error.kind === "cancelled")
        return;
      set((s) => ({
        stashes: {
          ...s.stashes,
          [repoId]:
            result.status === "ok"
              ? { entries: result.data, error: null, readAt: Date.now() }
              : {
                  entries: s.stashes[repoId]?.entries ?? null,
                  error: result.error,
                  readAt: s.stashes[repoId]?.readAt ?? Date.now(),
                },
        },
      }));
    },

    requestCreate: (repoId) => set({ dialog: { kind: "create", repoId } }),

    create: async (message, includeUntracked) => {
      const dialog = get().dialog;
      if (dialog?.kind !== "create") return;
      const { repoId } = dialog;
      set({ dialog: null, notice: null, busy: true });
      const result = await commands.createStash(
        repoId,
        message,
        includeUntracked,
      );
      set({ busy: false });
      afterWrite(repoId);
      if (result.status === "error") fail(repoId, result.error);
      else set({ notice: { kind: "created", repoId } });
    },

    apply: async (repoId, entry, mode, allowConflicts = false) => {
      set({ dialog: null, notice: null, busy: true });
      const result = await commands.applyStash(
        repoId,
        entry.oid,
        mode.pop,
        allowConflicts,
        mode.saveLocal,
      );
      set({ busy: false });
      afterWrite(repoId);
      if (result.status === "error") fail(repoId, result.error);
      else settle(repoId, entry, mode, result.data);
    },

    saveLocalAndApply: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "blocked") return;
      await get().apply(dialog.repoId, dialog.entry, {
        ...dialog.mode,
        saveLocal: true,
      });
    },

    applyWithConflicts: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "conflict") return;
      await get().apply(dialog.repoId, dialog.entry, dialog.mode, true);
    },

    requestRecover: async (repoId, entry, path) => {
      set({ notice: null });
      const result = await commands.recoverPreview(repoId, entry.oid, path);
      if (result.status === "error") {
        fail(repoId, result.error);
        return;
      }
      const preview = result.data;
      if (preview.identical) {
        set({ notice: { kind: "alreadyRecovered", repoId, path } });
      } else if (preview.lost === 0 && !preview.replacesWhole) {
        await recover(repoId, entry, preview);
      } else {
        set({ dialog: { kind: "recover", repoId, entry, preview } });
      }
    },

    confirmRecover: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "recover") return;
      set({ dialog: null });
      await recover(dialog.repoId, dialog.entry, dialog.preview);
    },

    requestDrop: (repoId, entry) =>
      set({ dialog: { kind: "drop", repoId, entry } }),

    confirmDrop: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "drop") return;
      const { repoId, entry } = dialog;
      set({ dialog: null, notice: null, busy: true });
      const result = await commands.dropStash(repoId, entry.oid);
      set({ busy: false });
      afterWrite(repoId);
      if (result.status === "error") fail(repoId, result.error);
      else {
        deselect(repoId, entry.oid);
        set({ notice: { kind: "dropped", repoId, entry } });
      }
    },

    closeDialog: () => set({ dialog: null }),
    dismissNotice: () => set({ notice: null }),

    forget: (repoId) =>
      set((s) => {
        latest.delete(repoId);
        const stashes = { ...s.stashes };
        delete stashes[repoId];
        return { stashes };
      }),
  };
});

/** The command that puts a dropped entry back. */
export function storeCommand(entry: StashEntry): string {
  const message = entry.message.replace(/(["\\$`])/g, "\\$1");
  return `git stash store -m "${message}" ${entry.oid}`;
}
