import { create } from "zustand";
import {
  commands,
  type CommandError,
  type LastFetch,
  type MoveKind,
  type MovePreview,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useHistoryStore } from "../history/store";
import { refreshAfterRemote, useRemoteStore } from "../remote/store";

/**
 * Merging a branch into another (task 8.2) or rebasing it onto another
 * (8.3): the dialog with git's preview, "Fetch first" for a remote branch,
 * and the result, which goes to the history notice (with "Undo" and
 * "Recover my changes").
 */

export type MergeDialogState = {
  kind: MoveKind;
  /** Opened by a drop (task 8.4): the dialog lets the user switch between
   * merge and rebase. */
  switchable: boolean;
  repoId: number;
  /** Full ref names. */
  source: string;
  target: string;
  preview: MovePreview | null;
  /** The repository changed since the preview the user saw. */
  changed: boolean;
  /** The remote of the remote branch taking part (the one that does not
   * move), and when it was last fetched. */
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
  /** Opens the dialog for rebasing `source` onto `target`. */
  requestRebase: (
    repoId: number,
    source: string,
    target: string,
  ) => Promise<void>;
  /** `source` was dropped on `target`: merge by default (it rewrites
   * nothing), rebase one click away. */
  requestDrop: (
    repoId: number,
    source: string,
    target: string,
  ) => Promise<void>;
  /** In a dialog opened by a drop: merge or rebase, same two branches. */
  switchKind: (kind: MoveKind) => Promise<void>;
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
    const previewOf =
      dialog.kind === "merge" ? commands.mergePreview : commands.rebasePreview;
    const [result, last] = await Promise.all([
      previewOf(repoId, source, target),
      dialog.remote
        ? commands.lastFetch(repoId, dialog.remote)
        : Promise.resolve(null),
    ]);
    const now = get().dialog;
    if (
      now?.repoId !== repoId ||
      now.source !== source ||
      now.kind !== dialog.kind
    ) {
      return;
    }
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

  /** The remote of the branch that does not move (the source of a merge,
   * the target of a rebase), if it is a remote branch. */
  const remoteFor = (
    kind: MoveKind,
    repoId: number,
    source: string,
    target: string,
  ) => {
    const fixed = kind === "merge" ? source : target;
    return fixed.startsWith("refs/remotes/")
      ? remoteOf(repoId, fixed.slice("refs/remotes/".length))
      : null;
  };

  const open = async (
    kind: MoveKind,
    repoId: number,
    source: string,
    target: string,
    switchable = false,
  ) => {
    useHistoryStore.setState({ notice: null });
    set({
      dialog: {
        kind,
        switchable,
        repoId,
        source,
        target,
        preview: null,
        changed: false,
        remote: remoteFor(kind, repoId, source, target),
        lastFetch: null,
        fetching: false,
      },
    });
    await load(false);
  };

  return {
    ...initialMergeState,

    requestMerge: (repoId, source, target) =>
      open("merge", repoId, source, target),
    requestRebase: (repoId, source, target) =>
      open("rebase", repoId, source, target),
    requestDrop: (repoId, source, target) =>
      open("merge", repoId, source, target, true),

    switchKind: async (kind) => {
      const dialog = get().dialog;
      if (!dialog?.switchable || dialog.kind === kind) return;
      const { repoId, source, target } = dialog;
      set({
        dialog: {
          ...dialog,
          kind,
          preview: null,
          changed: false,
          remote: remoteFor(kind, repoId, source, target),
          lastFetch: null,
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
      const common = {
        source: dialog.source,
        target: dialog.target,
        expected: preview.fingerprint,
        saveChanges: preview.blocked !== null,
        allowConflicts: preview.conflicts.length > 0,
      };
      const result =
        dialog.kind === "merge"
          ? await commands.mergeBranch(repoId, { ...common, noFf, message })
          : await commands.rebaseBranch(repoId, common);
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
          pick: dialog.kind,
          short: preview.source.name,
        });
      }
      switch (outcome.type) {
        case "done":
          set({ dialog: null });
          useHistoryStore.setState({
            notice: {
              kind: "merged",
              op: dialog.kind,
              repoId,
              undoId,
              count: preview.commitsTotal,
              published: preview.commits.some((c) => c.published),
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
