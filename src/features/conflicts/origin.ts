import { create } from "zustand";
import type { ConflictSides, InProgress } from "../../ipc";
import { useWorkdirStore } from "../workdir/store";

/**
 * Which of GitCRAFT's operations left a merge, rebase, cherry-pick or
 * revert waiting, so its notices say what really happened: git only knows
 * "a rebase", whether it came from a pull, a rebase of a branch (task 8)
 * or an interactive rebase (task 13).
 *
 * Remembered when an operation of the app stops on conflicts, and
 * forgotten once nothing is in progress. An operation started elsewhere
 * (the terminal) has no origin: its texts only name what git is doing.
 */
export type Origin =
  | "pullMerge"
  | "pullRebase"
  | "merge"
  | "rebase"
  | "interactiveRebase"
  | "cherryPick"
  | "revert";

const KIND: Record<Origin, InProgress> = {
  pullMerge: "merge",
  pullRebase: "rebase",
  merge: "merge",
  rebase: "rebase",
  interactiveRebase: "rebase",
  cherryPick: "cherryPick",
  revert: "revert",
};

type State = {
  origins: Record<number, Origin>;
  /** The status has shown it in progress since it was remembered: until
   * then, a status read before the operation does not count. */
  seen: Record<number, boolean>;
  remember: (repoId: number, origin: Origin) => void;
  forget: (repoId: number) => void;
};

export const initialOriginState = { origins: {}, seen: {} };

export const useOriginStore = create<State>()((set) => ({
  ...initialOriginState,
  remember: (repoId, origin) =>
    set((s) => ({
      origins: { ...s.origins, [repoId]: origin },
      seen: { ...s.seen, [repoId]: false },
    })),
  forget: (repoId) =>
    set((s) => {
      if (!(repoId in s.origins)) return s;
      const origins = { ...s.origins };
      const seen = { ...s.seen };
      delete origins[repoId];
      delete seen[repoId];
      return { origins, seen };
    }),
}));

/** Remembers that the app's own `origin` is now waiting in `repoId`. */
export function rememberOrigin(repoId: number, origin: Origin): void {
  useOriginStore.getState().remember(repoId, origin);
}

/** What is waiting in `repoId`, as precisely as is known: GitCRAFT's
 * interactive rebase leaves its own mark (`sides.interactive`); the rest
 * comes from what the app remembers, if it is the same kind of operation. */
export function originOf(
  repoId: number,
  operation: InProgress,
  sides: ConflictSides | null,
): Origin | null {
  if (sides?.type === "rebase" && sides.interactive) {
    return "interactiveRebase";
  }
  const remembered = useOriginStore.getState().origins[repoId];
  if (!remembered || KIND[remembered] !== operation) return null;
  // Not ours after all: GitCRAFT's interactive rebase always has its mark.
  if (remembered === "interactiveRebase" && sides?.type === "rebase") {
    return null;
  }
  return remembered;
}

// Once nothing is in progress (finished, aborted, here or elsewhere), the
// next operation may be someone else's.
useWorkdirStore.subscribe((state) => {
  const { origins, seen } = useOriginStore.getState();
  for (const key of Object.keys(origins)) {
    const repoId = Number(key);
    const status = state.statuses[repoId]?.status;
    if (!status) continue;
    if (status.inProgress) {
      if (!seen[repoId]) {
        useOriginStore.setState((s) => ({
          seen: { ...s.seen, [repoId]: true },
        }));
      }
    } else if (seen[repoId]) {
      useOriginStore.getState().forget(repoId);
    }
  }
});
