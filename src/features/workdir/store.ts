import { create } from "zustand";
import {
  commands,
  type CommandError,
  type DiscardedFile,
  type DiscardTarget,
  type HunkSelection,
  type SelectionAction,
  type WorkingStatus,
  type WorktreeArea,
} from "../../ipc";
import { useGraphStore } from "../graph/store";

export type RepoStatus = {
  status: WorkingStatus | null;
  error: CommandError | null;
  /** A stage/unstage/commit is running. */
  busy?: boolean;
  /** The last stage/unstage/commit failure, until the next action. */
  opError?: CommandError | null;
  /** Bumped on every successful status read (views re-read their diffs). */
  version?: number;
};

export type SelectionRequest = {
  path: string;
  oldPath: string | null;
  area: WorktreeArea;
  fingerprint: string;
  selection: HunkSelection[];
  action: SelectionAction;
};

/** Commit message being written, kept per repository across tab switches. */
export type Draft = { summary: string; description: string; amend: boolean };

const EMPTY_DRAFT: Draft = { summary: "", description: "", amend: false };

/** A discard waiting for the user's confirmation. */
export type PendingDiscard = {
  repoId: number;
  target: DiscardTarget;
  files: DiscardedFile[];
};

/** The last discard, undoable for a few seconds. */
export type LastDiscard = {
  repoId: number;
  undoId: number;
  files: DiscardedFile[];
  stashed: boolean;
  error: CommandError | null;
};

type WorkdirState = {
  statuses: Record<number, RepoStatus>;
  pendingDiscard: PendingDiscard | null;
  lastDiscard: LastDiscard | null;
  /** Shows what discarding `target` would lose and asks to confirm. */
  requestDiscard: (repoId: number, target: DiscardTarget) => Promise<void>;
  cancelDiscard: () => void;
  confirmDiscard: () => Promise<void>;
  undoLastDiscard: () => Promise<void>;
  dismissLastDiscard: () => void;
  drafts: Record<number, Draft>;
  /** Re-reads the working directory status (cheap; safe to call often). */
  refresh: (repoId: number) => Promise<void>;
  /** `paths: null` means every change. */
  stage: (repoId: number, paths: string[] | null) => Promise<void>;
  unstage: (repoId: number, paths: string[] | null) => Promise<void>;
  /** Stages or unstages selected hunks/lines of one file. */
  applySelection: (repoId: number, request: SelectionRequest) => Promise<void>;
  setDraft: (repoId: number, patch: Partial<Draft>) => void;
  /** Commits the staged changes (or amends) with the draft message; on
   * success clears the draft and selects the new commit. */
  commit: (repoId: number) => Promise<void>;
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
    drafts: {},
    pendingDiscard: null,
    lastDiscard: null,

    requestDiscard: async (repoId, target) => {
      const result = await commands.discardPreview(repoId, target);
      if (result.status === "ok") {
        set({ pendingDiscard: { repoId, target, files: result.data } });
      } else {
        update(repoId, { opError: result.error });
        await get().refresh(repoId);
      }
    },

    cancelDiscard: () => set({ pendingDiscard: null }),

    confirmDiscard: async () => {
      const pending = get().pendingDiscard;
      if (!pending) return;
      const { repoId, target } = pending;
      set({ pendingDiscard: null });
      update(repoId, { busy: true, opError: null });
      const result = await commands.discardChanges(repoId, target);
      update(repoId, {
        busy: false,
        opError: result.status === "error" ? result.error : null,
      });
      if (result.status === "ok") {
        set({
          lastDiscard: {
            repoId,
            undoId: result.data.undoId,
            files: result.data.files,
            stashed: result.data.stashed,
            error: null,
          },
        });
      }
      await get().refresh(repoId);
    },

    undoLastDiscard: async () => {
      const last = get().lastDiscard;
      if (!last) return;
      const result = await commands.undoDiscard(last.repoId, last.undoId);
      set({
        lastDiscard:
          result.status === "ok" ? null : { ...last, error: result.error },
      });
      await get().refresh(last.repoId);
    },

    dismissLastDiscard: () => set({ lastDiscard: null }),

    stage: (repoId, paths) =>
      run(repoId, () => commands.stageFiles(repoId, paths)),

    unstage: (repoId, paths) =>
      run(repoId, () => commands.unstageFiles(repoId, paths)),

    applySelection: (repoId, r) =>
      run(repoId, () =>
        commands.applySelection(
          repoId,
          r.path,
          r.oldPath,
          r.area,
          r.fingerprint,
          r.selection,
          r.action,
        ),
      ),

    setDraft: (repoId, patch) =>
      set((s) => ({
        drafts: {
          ...s.drafts,
          [repoId]: { ...EMPTY_DRAFT, ...s.drafts[repoId], ...patch },
        },
      })),

    commit: async (repoId) => {
      const draft = get().drafts[repoId] ?? EMPTY_DRAFT;
      update(repoId, { busy: true, opError: null });
      const result = await commands.createCommit(
        repoId,
        draft.summary,
        draft.description,
        draft.amend,
      );
      update(repoId, {
        busy: false,
        opError: result.status === "error" ? result.error : null,
      });
      if (result.status === "ok") {
        set((s) => ({ drafts: { ...s.drafts, [repoId]: EMPTY_DRAFT } }));
        const graph = useGraphStore.getState();
        graph.select(repoId, result.data);
        void graph.refresh(repoId);
      }
      await get().refresh(repoId);
    },

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
          ? {
              status: result.data,
              error: null,
              version: (get().statuses[repoId]?.version ?? 0) + 1,
            }
          : { error: result.error },
      );
    },

    forget: (repoId) =>
      set((s) => {
        latest.delete(repoId);
        const statuses = { ...s.statuses };
        delete statuses[repoId];
        const drafts = { ...s.drafts };
        delete drafts[repoId];
        return { statuses, drafts };
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
