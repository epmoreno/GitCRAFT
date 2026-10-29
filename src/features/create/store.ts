import { create } from "zustand";
import {
  commands,
  pickFolder,
  type CommandError,
  type InitCheck,
} from "../../ipc";
import { useRepoStore } from "../repo/store";

/** A "new repository" dialog on screen. */
export type CreateDialog =
  | { kind: "init"; path: string; check: InitCheck }
  | { kind: "clone"; parent: string };

type CreateState = {
  dialog: CreateDialog | null;
  busy: boolean;
  error: CommandError | null;
  /** Asks for a folder, then shows what `git init` would do there. */
  requestInit: (pickerTitle: string) => Promise<void>;
  confirmInit: (branch: string) => Promise<void>;
  /** Shows the clone dialog (URL, folder and name). */
  requestClone: () => Promise<void>;
  /** The chosen folder already is a repository: open it instead. */
  openInstead: () => Promise<void>;
  closeDialog: () => void;
};

export const initialCreateState = {
  dialog: null,
  busy: false,
  error: null,
} satisfies Partial<CreateState>;

export const useCreateStore = create<CreateState>()((set, get) => ({
  ...initialCreateState,

  requestInit: async (pickerTitle) => {
    const path = await pickFolder(pickerTitle);
    if (path === null) return;
    const result = await commands.checkInit(path);
    if (result.status === "ok") {
      set({ dialog: { kind: "init", path, check: result.data }, error: null });
    } else {
      useRepoStore.setState({ openError: { path, error: result.error } });
    }
  },

  confirmInit: async (branch) => {
    const dialog = get().dialog;
    if (dialog?.kind !== "init") return;
    set({ busy: true, error: null });
    const result = await commands.initRepository(dialog.path, branch);
    set({ busy: false });
    if (result.status === "ok") {
      set({ dialog: null });
      await useRepoStore.getState().adopt(result.data);
    } else {
      set({ error: result.error });
    }
  },

  requestClone: async () => {
    const result = await commands.cloneParent();
    set({
      dialog: {
        kind: "clone",
        parent: result.status === "ok" ? result.data : "",
      },
      error: null,
    });
  },

  openInstead: async () => {
    const dialog = get().dialog;
    if (dialog?.kind !== "init") return;
    set({ dialog: null });
    await useRepoStore.getState().openRepository(dialog.path);
  },

  closeDialog: () => set({ dialog: null, error: null }),
}));
