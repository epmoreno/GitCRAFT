import { create } from "zustand";
import { useWorkdirStore } from "../workdir/store";
import type { Choice, Choices } from "./result";

/**
 * Choices not saved yet, per repository and file, for as long as the
 * operation lasts: switching file or tab, or closing the view, keeps them.
 * Closing the tab or the window, aborting or continuing asks first (see
 * `unsavedPaths`). They live only in memory: they are never written to disk
 * unless the user saves.
 */
export type ConflictDraft = {
  /** Fingerprint of the file the choices were made on. */
  fingerprint: string | null;
  choices: Choices;
};

/** Closing something that would lose unsaved choices, waiting for the
 * user's answer. `paths` are "repo: file" for the window. */
export type Closing =
  | { kind: "tab"; repoId: number; paths: string[] }
  | { kind: "window"; paths: string[] };

type ConflictState = {
  drafts: Record<number, Record<string, ConflictDraft>>;
  closing: Closing | null;
  setClosing: (closing: Closing | null) => void;
  /** Sets (or clears, with `null`) the choice for block `index`. */
  setChoice: (
    repoId: number,
    path: string,
    fingerprint: string | null,
    index: number,
    choice: Choice | null,
  ) => void;
  clearDraft: (repoId: number, path: string) => void;
  /** Forgets every draft of the repository (operation ended or aborted). */
  clearRepo: (repoId: number) => void;
};

export const initialConflictState = {
  drafts: {} as Record<number, Record<string, ConflictDraft>>,
  closing: null as Closing | null,
};

export const useConflictStore = create<ConflictState>()((set) => ({
  ...initialConflictState,
  setClosing: (closing) => set({ closing }),
  setChoice: (repoId, path, fingerprint, index, choice) =>
    set((state) => {
      const repo = state.drafts[repoId] ?? {};
      const current = repo[path];
      // Choices made on another version of the file do not carry over.
      const base =
        current && current.fingerprint === fingerprint ? current.choices : {};
      const choices = { ...base };
      if (choice) choices[index] = choice;
      else delete choices[index];
      const next = { ...repo };
      if (Object.keys(choices).length === 0) delete next[path];
      else next[path] = { fingerprint, choices };
      return { drafts: { ...state.drafts, [repoId]: next } };
    }),
  clearDraft: (repoId, path) =>
    set((state) => {
      const repo = { ...(state.drafts[repoId] ?? {}) };
      delete repo[path];
      return { drafts: { ...state.drafts, [repoId]: repo } };
    }),
  clearRepo: (repoId) =>
    set((state) => {
      const drafts = { ...state.drafts };
      delete drafts[repoId];
      return { drafts };
    }),
}));

/** Files of the repository with choices not saved yet, sorted. */
export function unsavedPaths(repoId: number): string[] {
  const repo = useConflictStore.getState().drafts[repoId] ?? {};
  return Object.keys(repo).sort();
}

/** Every repository with unsaved choices. */
export function reposWithDrafts(): number[] {
  const drafts = useConflictStore.getState().drafts;
  return Object.keys(drafts)
    .map(Number)
    .filter((id) => Object.keys(drafts[id] ?? {}).length > 0);
}

// Choices belong to one operation: once it has ended (continued to the
// end, aborted, or finished in a terminal), forget them.
useWorkdirStore.subscribe((state) => {
  for (const repoId of reposWithDrafts()) {
    const status = state.statuses[repoId]?.status;
    if (status && status.inProgress === null) {
      useConflictStore.getState().clearRepo(repoId);
    }
  }
});
