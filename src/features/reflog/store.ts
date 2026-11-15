import { create } from "zustand";
import { commands, type CommandError, type Reflog } from "../../ipc";

/**
 * The history of a branch or of HEAD (task 11.5): where it has been,
 * from git's reflog. Read-only: its entries lead to the graph, to a new
 * branch or to the reset dialog (task 10.4).
 */

export type ReflogDialog = {
  repoId: number;
  /** `HEAD` or `refs/heads/…`. */
  refname: string;
  /** `null` while it loads. */
  reflog: Reflog | null;
  error: CommandError | null;
};

type ReflogState = {
  dialog: ReflogDialog | null;
  open: (repoId: number, refname: string) => Promise<void>;
  close: () => void;
};

export const initialReflogState = { dialog: null };

export const useReflogStore = create<ReflogState>()((set, get) => ({
  ...initialReflogState,

  open: async (repoId, refname) => {
    set({ dialog: { repoId, refname, reflog: null, error: null } });
    const result = await commands.branchReflog(repoId, refname);
    const now = get().dialog;
    if (now?.repoId !== repoId || now.refname !== refname) return;
    set({
      dialog:
        result.status === "ok"
          ? { ...now, reflog: result.data }
          : { ...now, error: result.error },
    });
  },

  close: () => set({ dialog: null }),
}));

/** `main` for `refs/heads/main`; `HEAD` stays. */
export function branchOf(refname: string): string {
  return refname.replace(/^refs\/heads\//, "");
}
