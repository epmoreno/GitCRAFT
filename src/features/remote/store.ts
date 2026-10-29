import { create } from "zustand";
import { commands, fetchRemote, type CommandError } from "../../ipc";
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
    }
  | { kind: "fetched"; repoId: number; remote: string | null };

type RemoteState = {
  running: RunningRemote | null;
  notice: RemoteNotice | null;
  /** Fetches from `remote` (every remote when `null`). */
  fetch: (
    repoId: number,
    remote: string | null,
    prune: boolean,
  ) => Promise<void>;
  cancel: () => Promise<void>;
  dismissNotice: () => void;
};

export const initialRemoteState = {
  running: null,
  notice: null,
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

  return {
    ...initialRemoteState,

    fetch: async (repoId, remote, prune) => {
      if (!begin(repoId, "fetch", remote)) return;
      const result = await fetchRemote(repoId, remote, prune, progress);
      set({ running: null });
      refreshAfterRemote(repoId);
      if (result.status === "ok") {
        set({ notice: { kind: "fetched", repoId, remote } });
      } else if (result.error.kind !== "cancelled") {
        set({
          notice: {
            kind: "error",
            repoId,
            operation: "fetch",
            error: result.error,
          },
        });
      }
    },

    cancel: async () => {
      const running = get().running;
      if (running) await commands.cancelOperation(running.repoId);
    },

    dismissNotice: () => set({ notice: null }),
  };
});
