import { create } from "zustand";
import { commands, type CommandError, type WorkingStatus } from "../../ipc";

export type RepoStatus = {
  status: WorkingStatus | null;
  error: CommandError | null;
};

type WorkdirState = {
  statuses: Record<number, RepoStatus>;
  /** Re-reads the working directory status (cheap; safe to call often). */
  refresh: (repoId: number) => Promise<void>;
  forget: (repoId: number) => void;
};

/** Latest request per repository: older answers are dropped. */
const latest = new Map<number, number>();
let nextRequest = 0;

export const useWorkdirStore = create<WorkdirState>()((set) => ({
  statuses: {},

  refresh: async (repoId) => {
    const request = ++nextRequest;
    latest.set(repoId, request);
    const result = await commands.workingStatus(repoId);
    if (latest.get(repoId) !== request) return;
    if (result.status === "error" && result.error.kind === "cancelled") return;
    set((s) => ({
      statuses: {
        ...s.statuses,
        [repoId]:
          result.status === "ok"
            ? { status: result.data, error: null }
            : {
                status: s.statuses[repoId]?.status ?? null,
                error: result.error,
              },
      },
    }));
  },

  forget: (repoId) =>
    set((s) => {
      latest.delete(repoId);
      const statuses = { ...s.statuses };
      delete statuses[repoId];
      return { statuses };
    }),
}));

export type ChangeCounts = {
  staged: number;
  unstaged: number;
  conflicted: number;
  total: number;
};

/** Counts for the "uncommitted changes" summary. Untracked files count as
 * unstaged: both still need staging. */
export function changeCounts(status: WorkingStatus | null): ChangeCounts {
  const staged = status?.staged.length ?? 0;
  const unstaged =
    (status?.unstaged.length ?? 0) + (status?.untracked.length ?? 0);
  const conflicted = status?.conflicted.length ?? 0;
  return {
    staged,
    unstaged,
    conflicted,
    total: staged + unstaged + conflicted,
  };
}
