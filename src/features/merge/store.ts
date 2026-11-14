import { create } from "zustand";
import {
  commands,
  type CommandError,
  type LastFetch,
  type MovePreview,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useHistoryStore } from "../history/store";
import { refreshAfterRemote, useRemoteStore } from "../remote/store";

/**
 * Merging a branch into another (task 8.2): the dialog with git's preview,
 * "Fetch first" for a remote branch, and the result, which goes to the
 * history notice (with "Undo" and "Recover my changes").
 */

export type MergeDialogState = {
  repoId: number;
  /** Full ref names. */
  source: string;
  target: string;
  preview: MovePreview | null;
  /** The repository changed since the preview the user saw. */
  changed: boolean;
  /** The remote of a remote source branch, and when it was last fetched. */
  remote: string | null;
  lastFetch: LastFetch | null;
  fetching: boolean;
};

type MergeState = {
  dialog: MergeDialogState | null;
  busy: boolean;
  /** Opens the dialog for merging `source` into `target` (full ref names). */
  requestMerge: (
    repoId: number,
    source: string,
    target: string,
  ) => Promise<void>;
  /** Fetches the source's remote, then shows the preview again. */
  fetchFirst: () => Promise<void>;
  confirm: (options: {
    noFf: boolean;
    message: string | null;
  }) => Promise<void>;
  close: () => void;
};

export const initialMergeState = { dialog: null, busy: false };

/** The configured remote a remote branch's short name belongs to. */
export function remoteOf(repoId: number, shortName: string): string | null {
  const remotes = useGraphStore.getState().graphs[repoId]?.refs?.remotes ?? [];
  return (
    remotes
      .filter((r) => shortName.startsWith(`${r}/`))
      .sort((a, b) => b.length - a.length)[0] ?? null
  );
}

function showError(repoId: number, error: CommandError) {
  useHistoryStore.setState({ notice: { kind: "error", repoId, error } });
}

export const useMergeStore = create<MergeState>()((set, get) => {
  /** Loads the preview (and the last fetch) for the open dialog. */
  const load = async (changed: boolean) => {
    const dialog = get().dialog;
    if (!dialog) return;
    const { repoId, source, target } = dialog;
    const [result, last] = await Promise.all([
      commands.mergePreview(repoId, source, target),
      dialog.remote
        ? commands.lastFetch(repoId, dialog.remote)
        : Promise.resolve(null),
    ]);
    const now = get().dialog;
    if (now?.repoId !== repoId || now.source !== source) return;
    if (result.status === "error") {
      set({ dialog: null });
      showError(repoId, result.error);
      return;
    }
    set({
      dialog: {
        ...now,
        preview: result.data,
        changed,
        lastFetch: last?.status === "ok" ? last.data : now.lastFetch,
      },
    });
  };

  return {
    ...initialMergeState,

    requestMerge: async (repoId, source, target) => {
      useHistoryStore.setState({ notice: null });
      const remoteBranch = source.startsWith("refs/remotes/")
        ? source.slice("refs/remotes/".length)
        : null;
      set({
        dialog: {
          repoId,
          source,
          target,
          preview: null,
          changed: false,
          remote: remoteBranch ? remoteOf(repoId, remoteBranch) : null,
          lastFetch: null,
          fetching: false,
        },
      });
      await load(false);
    },

    fetchFirst: async () => {
      const dialog = get().dialog;
      if (!dialog?.remote) return;
      set({ dialog: { ...dialog, fetching: true } });
      await useRemoteStore
        .getState()
        .fetch(dialog.repoId, dialog.remote, false);
      const now = get().dialog;
      if (now !== null) set({ dialog: { ...now, fetching: false } });
      await load(false);
    },

    confirm: async ({ noFf, message }) => {
      const dialog = get().dialog;
      const preview = dialog?.preview;
      if (!dialog || !preview) return;
      const { repoId } = dialog;
      set({ busy: true });
      const result = await commands.mergeBranch(repoId, {
        source: dialog.source,
        target: dialog.target,
        expected: preview.fingerprint,
        noFf,
        message,
        saveChanges: preview.blocked !== null,
        allowConflicts: preview.conflicts.length > 0,
      });
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (result.status === "error") {
        set({ dialog: null });
        showError(repoId, result.error);
        return;
      }
      const { outcome, undoId } = result.data;
      const history = useHistoryStore.getState();
      if (outcome.saved) {
        history.rememberSaved(repoId, {
          oid: outcome.saved,
          pick: "merge",
          short: preview.source.name,
        });
      }
      switch (outcome.type) {
        case "done":
          set({ dialog: null });
          useHistoryStore.setState({
            notice: {
              kind: "merged",
              repoId,
              undoId,
              source: preview.source.name,
              target: preview.target.name,
              fastForward: outcome.fastForward,
              undoGone: false,
            },
          });
          void useGraphStore
            .getState()
            .refresh(repoId)
            .then(() => useGraphStore.getState().select(repoId, outcome.head));
          return;
        case "conflicted":
          // The banner and the conflict view (task 9) take over.
          set({ dialog: null });
          return;
        case "notDone":
          set({
            dialog: {
              ...dialog,
              preview: outcome.preview,
              changed: outcome.changed,
            },
          });
          return;
      }
    },

    close: () => set({ dialog: null }),
  };
});
