import { create } from "zustand";
import {
  commands,
  type BlockingFile,
  type BranchDeletion,
  type CheckoutOutcome,
  type CommandError,
  type GitRef,
  type RepoRefs,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";

/** The branch dialog on screen, if any. */
export type BranchDialog =
  | {
      kind: "create";
      repoId: number;
      /** A commit id or a full ref name. */
      start: string;
      /** How the start point is shown: a branch, tag or short commit id. */
      startLabel: string;
      /** Set the start point (a remote branch) as upstream. */
      track: boolean;
      /** Prefilled name. */
      name: string;
    }
  | { kind: "rename"; repoId: number; name: string }
  | { kind: "delete"; repoId: number; preview: BranchDeletion }
  | {
      kind: "blocked";
      repoId: number;
      name: string;
      files: BlockingFile[];
      changed: number;
    };

/** What the branch toast reports after an operation. */
export type BranchNotice =
  | {
      kind: "error";
      repoId: number;
      error: CommandError;
      /** The failed operation started by stashing for this branch. */
      stashFor?: string;
    }
  | { kind: "stashed"; repoId: number; name: string }
  | { kind: "deleted"; repoId: number; name: string; tip: string };

type BranchState = {
  dialog: BranchDialog | null;
  notice: BranchNotice | null;
  busy: boolean;
  /** Switches to a local branch; explains (and offers to stash) when
   * uncommitted changes are in the way. */
  checkout: (repoId: number, name: string) => Promise<void>;
  /** Switches to a local branch tracking `remote`, creating it if needed. */
  checkoutRemote: (
    repoId: number,
    remote: GitRef,
    refs: RepoRefs,
  ) => Promise<void>;
  requestCreate: (
    repoId: number,
    start: string,
    startLabel: string,
    options?: { track?: boolean; name?: string },
  ) => void;
  create: (name: string, checkout: boolean) => Promise<void>;
  requestRename: (repoId: number, name: string) => void;
  rename: (newName: string) => Promise<void>;
  /** Loads what deleting `name` would leave without a branch, then asks. */
  requestDelete: (repoId: number, name: string) => Promise<void>;
  confirmDelete: () => Promise<void>;
  stashAndSwitch: () => Promise<void>;
  closeDialog: () => void;
  dismissNotice: () => void;
};

export const initialBranchState = {
  dialog: null,
  notice: null,
  busy: false,
} satisfies Partial<BranchState>;

/** History and status after a branch operation (also when it failed). */
function refreshRepo(repoId: number) {
  void useGraphStore.getState().refresh(repoId);
  void useWorkdirStore.getState().refresh(repoId);
}

export const useBranchStore = create<BranchState>()((set, get) => {
  const fail = (repoId: number, error: CommandError) =>
    set({ notice: { kind: "error", repoId, error } });

  /** Shows the outcome of a switch: done, or blocked by local changes. */
  const settle = (repoId: number, name: string, outcome: CheckoutOutcome) => {
    if (outcome.type === "blocked") {
      set({
        dialog: {
          kind: "blocked",
          repoId,
          name,
          files: outcome.files,
          changed: outcome.changed,
        },
      });
    }
  };

  const write = async <T>(
    repoId: number,
    action: () => Promise<
      { status: "ok"; data: T } | { status: "error"; error: CommandError }
    >,
  ): Promise<T | undefined> => {
    set({ busy: true });
    const result = await action();
    set({ busy: false });
    refreshRepo(repoId);
    if (result.status === "error") {
      fail(repoId, result.error);
      return undefined;
    }
    return result.data;
  };

  return {
    ...initialBranchState,

    checkout: async (repoId, name) => {
      set({ notice: null });
      const outcome = await write(repoId, () =>
        commands.checkoutBranch(repoId, name),
      );
      if (outcome) settle(repoId, name, outcome);
    },

    checkoutRemote: async (repoId, remote, refs) => {
      const tracking = refs.refs.find(
        (r) => r.kind === "local" && r.upstream === remote.shortName,
      );
      if (tracking) return get().checkout(repoId, tracking.shortName);
      const name = remote.remote
        ? remote.shortName.slice(remote.remote.length + 1)
        : remote.shortName;
      const check = await commands.checkBranchName(repoId, name);
      if (check.status === "ok" && check.data === "valid") {
        set({ notice: null });
        const outcome = await write(repoId, () =>
          commands.createBranch(repoId, name, remote.name, true, true),
        );
        if (outcome) settle(repoId, name, outcome);
        return;
      }
      // The name is taken by an unrelated branch: let the user pick one.
      get().requestCreate(repoId, remote.name, remote.shortName, {
        track: true,
        name,
      });
    },

    requestCreate: (repoId, start, startLabel, options = {}) =>
      set({
        dialog: {
          kind: "create",
          repoId,
          start,
          startLabel,
          track: options.track ?? false,
          name: options.name ?? "",
        },
      }),

    create: async (name, checkout) => {
      const dialog = get().dialog;
      if (dialog?.kind !== "create") return;
      const { repoId, start, track } = dialog;
      set({ dialog: null, notice: null });
      const outcome = await write(repoId, () =>
        commands.createBranch(repoId, name, start, track, checkout),
      );
      if (outcome) settle(repoId, name, outcome);
    },

    requestRename: (repoId, name) =>
      set({ dialog: { kind: "rename", repoId, name } }),

    rename: async (newName) => {
      const dialog = get().dialog;
      if (dialog?.kind !== "rename") return;
      set({ dialog: null, notice: null });
      await write(dialog.repoId, () =>
        commands.renameBranch(dialog.repoId, dialog.name, newName),
      );
    },

    requestDelete: async (repoId, name) => {
      const result = await commands.branchDeletionPreview(repoId, name);
      if (result.status === "ok") {
        set({ dialog: { kind: "delete", repoId, preview: result.data } });
      } else {
        fail(repoId, result.error);
        refreshRepo(repoId);
      }
    },

    confirmDelete: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "delete") return;
      const { repoId, preview } = dialog;
      set({ dialog: null, notice: null });
      const done = await write(repoId, () =>
        commands.deleteBranch(repoId, preview.name, preview.tip),
      );
      if (done !== undefined) {
        set({
          notice: {
            kind: "deleted",
            repoId,
            name: preview.name,
            tip: preview.tip,
          },
        });
      }
    },

    stashAndSwitch: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "blocked") return;
      const { repoId, name, files } = dialog;
      set({ dialog: null, notice: null });
      const outcome = await write(repoId, () =>
        commands.stashAndCheckout(
          repoId,
          name,
          files.some((f) => f.untracked),
        ),
      );
      if (!outcome) {
        const notice = get().notice;
        if (notice?.kind === "error")
          set({ notice: { ...notice, stashFor: name } });
        return;
      }
      if (outcome.type === "switched") {
        set({ notice: { kind: "stashed", repoId, name } });
      } else {
        settle(repoId, name, outcome);
      }
    },

    closeDialog: () => set({ dialog: null }),
    dismissNotice: () => set({ notice: null }),
  };
});
