import { create } from "zustand";
import {
  commands,
  fetchRemote,
  pullBranch,
  type BlockingFile,
  type CommandError,
  type InProgress,
  type PullBlock,
  type PullMode,
  type PullModeInfo,
} from "../../ipc";
import { progressPercent } from "../graph/OptimizeNotice";
import { useGraphStore } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";

export type RemoteKind = "fetch" | "pull" | "push";

/** A remote operation in progress (one per window at a time). */
export type RunningRemote = {
  repoId: number;
  kind: RemoteKind;
  /** Remote it talks to, when only one. */
  remote: string | null;
  /** git's last progress line, as is. */
  line: string;
  percent: number | null;
};

/** What the remote toast reports after an operation. */
export type RemoteNotice =
  | {
      kind: "error";
      repoId: number;
      operation: RemoteKind;
      error: CommandError;
      /** Uncommitted changes were stashed before it failed. */
      savedLocal?: boolean;
    }
  | { kind: "fetched"; repoId: number; remote: string | null }
  | {
      kind: "pulled";
      repoId: number;
      upstream: string;
      commits: number;
      mode: PullMode;
      fastForward: boolean;
      savedLocal: boolean;
    }
  | { kind: "upToDate"; repoId: number; upstream: string; savedLocal: boolean }
  | { kind: "noUpstream"; repoId: number; branch: string }
  | { kind: "upstreamGone"; repoId: number; upstream: string }
  | {
      kind: "pullConflicted";
      repoId: number;
      files: string[];
      mode: PullMode;
      savedLocal: boolean;
    }
  | { kind: "aborted"; repoId: number; operation: InProgress };

/** A remote dialog waiting for the user. */
export type RemoteDialog =
  | {
      kind: "pullBlocked";
      repoId: number;
      files: BlockingFile[];
      reason: PullBlock;
    }
  | {
      kind: "pullConflict";
      repoId: number;
      upstream: string;
      files: string[];
      exact: boolean;
    }
  | { kind: "abort"; repoId: number; operation: InProgress };

type RemoteState = {
  running: RunningRemote | null;
  notice: RemoteNotice | null;
  dialog: RemoteDialog | null;
  /** Pull mode in effect, per repository. */
  pullModes: Record<number, PullModeInfo>;
  /** Fetches from `remote` (every remote when `null`). */
  fetch: (
    repoId: number,
    remote: string | null,
    prune: boolean,
  ) => Promise<void>;
  loadPullMode: (repoId: number) => Promise<void>;
  /** `null`: follow git's configuration again. */
  setPullMode: (repoId: number, mode: PullMode | null) => Promise<void>;
  pull: (
    repoId: number,
    options?: { allowConflicts?: boolean; saveLocal?: boolean },
  ) => Promise<void>;
  /** From the "blocked" dialog: stash uncommitted changes, then pull. */
  saveLocalAndPull: () => Promise<void>;
  /** From the "conflict" dialog: pull anyway, stopping on conflicts. */
  pullWithConflicts: () => Promise<void>;
  requestAbort: (repoId: number, operation: InProgress) => void;
  confirmAbort: () => Promise<void>;
  closeDialog: () => void;
  cancel: () => Promise<void>;
  dismissNotice: () => void;
};

export const initialRemoteState = {
  running: null,
  notice: null,
  dialog: null,
  pullModes: {},
} satisfies Partial<RemoteState>;

/** History, refs and status after any remote operation. */
export function refreshAfterRemote(repoId: number) {
  void useGraphStore.getState().refresh(repoId);
  void useWorkdirStore.getState().refresh(repoId);
}

export const useRemoteStore = create<RemoteState>()((set, get) => {
  /** Starts tracking an operation; false if another one is running. */
  const begin = (repoId: number, kind: RemoteKind, remote: string | null) => {
    if (get().running) return false;
    set({
      running: { repoId, kind, remote, line: "", percent: null },
      notice: null,
      dialog: null,
    });
    return true;
  };

  const progress = (line: string) =>
    set((s) =>
      s.running
        ? {
            running: {
              ...s.running,
              line,
              percent: progressPercent(line) ?? s.running.percent,
            },
          }
        : s,
    );

  const notify = (notice: RemoteNotice) => set({ notice });

  return {
    ...initialRemoteState,

    fetch: async (repoId, remote, prune) => {
      if (!begin(repoId, "fetch", remote)) return;
      const result = await fetchRemote(repoId, remote, prune, progress);
      set({ running: null });
      refreshAfterRemote(repoId);
      if (result.status === "ok") {
        notify({ kind: "fetched", repoId, remote });
      } else if (result.error.kind !== "cancelled") {
        notify({
          kind: "error",
          repoId,
          operation: "fetch",
          error: result.error,
        });
      }
    },

    loadPullMode: async (repoId) => {
      const result = await commands.getPullMode(repoId);
      if (result.status === "ok") {
        set((s) => ({ pullModes: { ...s.pullModes, [repoId]: result.data } }));
      }
    },

    setPullMode: async (repoId, mode) => {
      await commands.setPullMode(repoId, mode);
      await get().loadPullMode(repoId);
    },

    pull: async (repoId, options = {}) => {
      const allowConflicts = options.allowConflicts ?? false;
      const saveLocal = options.saveLocal ?? false;
      if (!begin(repoId, "pull", null)) return;
      const result = await pullBranch(
        repoId,
        { allowConflicts, saveLocal },
        progress,
      );
      set({ running: null });
      refreshAfterRemote(repoId);
      if (result.status === "error") {
        // A cancelled pull says nothing, unless changes were stashed.
        if (result.error.kind !== "cancelled" || saveLocal) {
          notify({
            kind: "error",
            repoId,
            operation: "pull",
            error: result.error,
            savedLocal: saveLocal,
          });
        }
        return;
      }
      const outcome = result.data;
      switch (outcome.type) {
        case "pulled":
          notify({
            kind: "pulled",
            repoId,
            upstream: outcome.upstream,
            commits: outcome.commits,
            mode: outcome.mode,
            fastForward: outcome.fastForward,
            savedLocal: saveLocal,
          });
          break;
        case "upToDate":
          notify({
            kind: "upToDate",
            repoId,
            upstream: outcome.upstream,
            savedLocal: saveLocal,
          });
          break;
        case "noUpstream":
          notify({ kind: "noUpstream", repoId, branch: outcome.branch });
          break;
        case "upstreamGone":
          notify({ kind: "upstreamGone", repoId, upstream: outcome.upstream });
          break;
        case "blocked":
          set({
            dialog: {
              kind: "pullBlocked",
              repoId,
              files: outcome.files,
              reason: outcome.reason,
            },
          });
          break;
        case "wouldConflict":
          set({
            dialog: {
              kind: "pullConflict",
              repoId,
              upstream: outcome.upstream,
              files: outcome.files,
              exact: outcome.exact,
            },
          });
          break;
        case "conflicted":
          notify({
            kind: "pullConflicted",
            repoId,
            files: outcome.files,
            mode: outcome.mode,
            savedLocal: saveLocal,
          });
          break;
      }
    },

    saveLocalAndPull: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "pullBlocked") return;
      await get().pull(dialog.repoId, { saveLocal: true });
    },

    pullWithConflicts: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "pullConflict") return;
      await get().pull(dialog.repoId, { allowConflicts: true });
    },

    requestAbort: (repoId, operation) =>
      set({ dialog: { kind: "abort", repoId, operation } }),

    confirmAbort: async () => {
      const dialog = get().dialog;
      if (dialog?.kind !== "abort") return;
      const { repoId, operation } = dialog;
      set({ dialog: null, notice: null });
      const result = await commands.abortInProgress(repoId);
      refreshAfterRemote(repoId);
      notify(
        result.status === "ok"
          ? { kind: "aborted", repoId, operation }
          : { kind: "error", repoId, operation: "pull", error: result.error },
      );
    },

    closeDialog: () => set({ dialog: null }),

    cancel: async () => {
      const running = get().running;
      if (running) await commands.cancelOperation(running.repoId);
    },

    dismissNotice: () => set({ notice: null }),
  };
});
