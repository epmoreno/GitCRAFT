import { create } from "zustand";
import { commands, type CommandError, type WorkingStatus } from "../../ipc";

export type RepoStatus = {
  status: WorkingStatus | null;
  error: CommandError | null;
  /** A stage/unstage/commit is running. */
  busy?: boolean;
  /** The last stage/unstage/commit failure, until the next action. */
  opError?: CommandError | null;
};

type WorkdirState = {
  statuses: Record<number, RepoStatus>;
  /** Re-reads the working directory status (cheap; safe to call often). */
  refresh: (repoId: number) => Promise<void>;
  /** `paths: null` means every change. */
  stage: (repoId: number, paths: string[] | null) => Promise<void>;
  unstage: (repoId: number, paths: string[] | null) => Promise<void>;
  dismissError: (repoId: number) => void;
  forget: (repoId: number) => void;
};

/** Latest request per repository: older answers are dropped. */
const latest = new Map<number, number>();
let nextRequest = 0;

export const useWorkdirStore = create<WorkdirState>()((set, get) => {
  const update = (repoId: number, patch: Partial<RepoStatus>) =>
    set((s) => ({
      statuses: {
        ...s.statuses,
        [repoId]: {
          status: null,
          error: null,
          ...s.statuses[repoId],
          ...patch,
        },
      },
    }));

  /** Runs a write, then re-reads the status whatever the outcome. */
  const run = async (
    repoId: number,
    action: () => Promise<
      { status: "ok" } | { status: "error"; error: CommandError }
    >,
  ) => {
    update(repoId, { busy: true, opError: null });
    const result = await action();
    update(repoId, {
      busy: false,
      opError: result.status === "error" ? result.error : null,
    });
    await get().refresh(repoId);
  };

  return {
    statuses: {},

    stage: (repoId, paths) =>
      run(repoId, () => commands.stageFiles(repoId, paths)),

    unstage: (repoId, paths) =>
      run(repoId, () => commands.unstageFiles(repoId, paths)),

    dismissError: (repoId) => update(repoId, { opError: null }),

    refresh: async (repoId) => {
      const request = ++nextRequest;
      latest.set(repoId, request);
      const result = await commands.workingStatus(repoId);
      if (latest.get(repoId) !== request) return;
      if (result.status === "error" && result.error.kind === "cancelled")
        return;
      update(
        repoId,
        result.status === "ok"
          ? { status: result.data, error: null }
          : { error: result.error },
      );
    },

    forget: (repoId) =>
      set((s) => {
        latest.delete(repoId);
        const statuses = { ...s.statuses };
        delete statuses[repoId];
        return { statuses };
      }),
  };
});

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
