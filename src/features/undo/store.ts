import { create } from "zustand";
import {
  commands,
  type Direction,
  type UndoPreview,
  type UndoState,
} from "../../ipc";
import { useHistoryStore } from "../history/store";
import { refreshAfterRemote } from "../remote/store";

/**
 * Undo and redo from the toolbar and the shortcuts (task 11.4): always
 * through a dialog with the preview of what will be restored. The result
 * is shown in the history notice, with the way back.
 */

export type UndoDialog = {
  repoId: number;
  direction: Direction;
  /** The stack entry. */
  entry: number;
  preview: UndoPreview;
  /** The repository changed since the preview the user saw: nothing was
   * done and this is the new one. */
  changed: boolean;
};

type UndoStoreState = {
  /** What the toolbar offers, per repository. */
  tops: Record<number, UndoState>;
  dialog: UndoDialog | null;
  busy: boolean;
  refresh: (repoId: number) => Promise<void>;
  /** Opens the dialog for the top entry of `direction`'s stack. */
  request: (repoId: number, direction: Direction) => Promise<void>;
  /** Does it; `saveChanges` stashes the changes in the way first. */
  confirm: (saveChanges: boolean) => Promise<void>;
  /** Published commits in the way: revert the one instead (task 10). */
  revertInstead: (oid: string) => void;
  close: () => void;
};

export const initialUndoState = {
  tops: {},
  dialog: null,
  busy: false,
};

const opposite = (direction: Direction): Direction =>
  direction === "undo" ? "redo" : "undo";

export const useUndoStore = create<UndoStoreState>()((set, get) => {
  const fail = (repoId: number, error: Parameters<typeof notifyError>[1]) =>
    notifyError(repoId, error);

  return {
    ...initialUndoState,

    refresh: async (repoId) => {
      const result = await commands.undoState(repoId);
      if (result.status !== "ok") return;
      set((s) => ({ tops: { ...s.tops, [repoId]: result.data } }));
    },

    request: async (repoId, direction) => {
      if (get().dialog || get().busy) return;
      const state = await commands.undoState(repoId);
      if (state.status === "error") {
        fail(repoId, state.error);
        return;
      }
      set((s) => ({ tops: { ...s.tops, [repoId]: state.data } }));
      const top = state.data[direction];
      if (!top) return;
      const preview = await commands.undoRedoPreview(repoId, top.id, direction);
      if (preview.status === "error") {
        fail(repoId, preview.error);
        return;
      }
      useHistoryStore.getState().dismissNotice();
      set({
        dialog: {
          repoId,
          direction,
          entry: top.id,
          preview: preview.data,
          changed: false,
        },
      });
    },

    confirm: async (saveChanges) => {
      const dialog = get().dialog;
      if (!dialog || get().busy) return;
      const { repoId, direction, entry, preview } = dialog;
      set({ busy: true });
      const result = await commands.undoRedo(repoId, entry, {
        direction,
        expected: preview.fingerprint,
        saveChanges,
      });
      set({ busy: false });
      refreshAfterRemote(repoId);
      void get().refresh(repoId);
      if (result.status === "error") {
        set({ dialog: null });
        fail(repoId, result.error);
        return;
      }
      if (result.data.type === "notDone") {
        set({
          dialog: { ...dialog, preview: result.data.preview, changed: true },
        });
        return;
      }
      const { saved, unstaged, exact } = result.data;
      const history = useHistoryStore.getState();
      // The operation's own copy came back: nothing left to recover.
      if (exact && preview.restoresStash) {
        if (history.saved[repoId]?.oid === preview.restoresStash) {
          history.forgetSaved(repoId);
        }
      }
      if (saved) {
        history.rememberSaved(repoId, {
          oid: saved,
          pick: direction,
          short: "",
        });
      }
      set({ dialog: null });
      useHistoryStore.setState({
        notice: {
          kind: "undoDone",
          repoId,
          direction,
          op: preview.kind,
          unstaged,
        },
      });
    },

    revertInstead: (oid) => {
      const dialog = get().dialog;
      if (!dialog) return;
      set({ dialog: null });
      void useHistoryStore.getState().requestPick(dialog.repoId, oid, "revert");
    },

    close: () => set({ dialog: null }),
  };
});

function notifyError(
  repoId: number,
  error: Extract<
    Awaited<ReturnType<typeof commands.undoState>>,
    { status: "error" }
  >["error"],
) {
  useHistoryStore.setState({ notice: { kind: "error", repoId, error } });
}

/** The way back from a notice: the dialog for the opposite direction. */
export function requestOpposite(repoId: number, direction: Direction) {
  void useUndoStore.getState().request(repoId, opposite(direction));
}
