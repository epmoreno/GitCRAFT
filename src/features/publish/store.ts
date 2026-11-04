import { create } from "zustand";
import {
  commands,
  type CommandError,
  type PublishInfo,
  type Published,
} from "../../ipc";
import { useRemoteStore } from "../remote/store";

/** Where publishing is. */
export type PublishStep =
  /** Filling in the form. */
  | { kind: "form" }
  /** Checking the name and creating the repository on GitHub. */
  | { kind: "creating" }
  /** Created and `origin` added; pushing the branch. */
  | { kind: "pushing"; published: Published }
  | { kind: "done"; published: Published }
  /** Created and `origin` added, but the push failed: Push retries it. */
  | { kind: "pushFailed"; published: Published; error: CommandError | null };

type PublishDialog = {
  repoId: number;
  info: PublishInfo | null;
  /** Reading what would be published failed, or publishing failed before
   * anything was created. */
  error: CommandError | null;
  step: PublishStep;
};

type PublishState = {
  dialog: PublishDialog | null;
  open: (repoId: number) => Promise<void>;
  /** Re-reads the session (after signing in from Settings). */
  reload: () => Promise<void>;
  publish: (
    name: string,
    description: string,
    isPrivate: boolean,
  ) => Promise<void>;
  close: () => void;
};

export const initialPublishState = { dialog: null };

export const usePublishStore = create<PublishState>()((set, get) => {
  const load = async (repoId: number) => {
    const result = await commands.publishInfo(repoId);
    set((s) =>
      s.dialog?.repoId === repoId
        ? {
            dialog: {
              ...s.dialog,
              info: result.status === "ok" ? result.data : s.dialog.info,
              error: result.status === "ok" ? null : result.error,
            },
          }
        : s,
    );
  };
  const step = (
    repoId: number,
    next: PublishStep,
    error: CommandError | null = null,
  ) =>
    set((s) =>
      s.dialog?.repoId === repoId
        ? { dialog: { ...s.dialog, step: next, error } }
        : s,
    );

  return {
    ...initialPublishState,

    open: async (repoId) => {
      set({
        dialog: { repoId, info: null, error: null, step: { kind: "form" } },
      });
      await load(repoId);
    },

    reload: async () => {
      const dialog = get().dialog;
      if (dialog) await load(dialog.repoId);
    },

    publish: async (name, description, isPrivate) => {
      const dialog = get().dialog;
      if (!dialog || dialog.step.kind !== "form") return;
      const { repoId } = dialog;
      step(repoId, { kind: "creating" });
      const created = await commands.publishRepository(
        repoId,
        name.trim(),
        description.trim() === "" ? null : description.trim(),
        isPrivate,
      );
      if (created.status === "error") {
        // Nothing was created: back to the form, with the reason.
        step(repoId, { kind: "form" }, created.error);
        return;
      }
      const published = created.data;
      step(repoId, { kind: "pushing", published });
      // The usual push: publishes the branch to origin with its upstream,
      // with progress and Cancel. Never forced.
      const remote = useRemoteStore.getState();
      await remote.push(repoId, { publishTo: "origin" });
      const notice = useRemoteStore.getState().notice;
      if (notice?.repoId === repoId && notice.kind === "pushed") {
        step(repoId, { kind: "done", published });
      } else {
        step(repoId, {
          kind: "pushFailed",
          published,
          error:
            notice?.repoId === repoId && notice.kind === "error"
              ? notice.error
              : null,
        });
      }
    },

    close: () => set({ dialog: null }),
  };
});
