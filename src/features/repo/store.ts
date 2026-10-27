import { create } from "zustand";
import { useGraphStore } from "../graph/store";
import {
  commands,
  pickFolder,
  type CommandError,
  type OpenedRepository,
  type RecentRepository,
} from "../../ipc";

export type OpenError = {
  /** The folder the user tried to open. */
  path: string;
  error: CommandError;
};

type RepoState = {
  tabs: OpenedRepository[];
  activeId: number | null;
  recents: RecentRepository[];
  opening: boolean;
  openError: OpenError | null;

  loadRecents: () => Promise<void>;
  openRepository: (path: string) => Promise<void>;
  /** Shows the folder picker and opens the chosen folder. */
  pickAndOpen: (dialogTitle: string) => Promise<void>;
  activate: (id: number) => void;
  closeTab: (id: number) => Promise<void>;
  dismissError: () => void;
};

export const initialRepoState = {
  tabs: [],
  activeId: null,
  recents: [],
  opening: false,
  openError: null,
} satisfies Partial<RepoState>;

export const useRepoStore = create<RepoState>()((set, get) => ({
  ...initialRepoState,

  loadRecents: async () => {
    const result = await commands.listRecentRepositories();
    if (result.status === "ok") set({ recents: result.data });
  },

  openRepository: async (path) => {
    set({ opening: true, openError: null });
    const result = await commands.openRepository(path);
    if (result.status === "ok") {
      const opened = result.data;
      const alreadyOpen = get().tabs.some((t) => t.id === opened.id);
      set((s) => ({
        tabs: alreadyOpen ? s.tabs : [...s.tabs, opened],
        activeId: opened.id,
        opening: false,
      }));
    } else {
      set({ opening: false, openError: { path, error: result.error } });
    }
    // Opening updates the list; a failure may have revealed a deleted folder.
    await get().loadRecents();
  },

  pickAndOpen: async (dialogTitle) => {
    const path = await pickFolder(dialogTitle);
    if (path !== null) await get().openRepository(path);
  },

  activate: (id) => set({ activeId: id }),

  closeTab: async (id) => {
    const { tabs, activeId } = get();
    const index = tabs.findIndex((t) => t.id === id);
    if (index === -1) return;
    const remaining = tabs.filter((t) => t.id !== id);
    // Closing the active tab selects its right neighbour, else the left one.
    const nextActive =
      activeId === id
        ? ((remaining[index] ?? remaining[index - 1] ?? null)?.id ?? null)
        : activeId;
    set({ tabs: remaining, activeId: nextActive });
    useGraphStore.getState().forget(id);
    await commands.closeRepository(id);
  },

  dismissError: () => set({ openError: null }),
}));

export function useActiveRepository(): OpenedRepository | null {
  return useRepoStore((s) => s.tabs.find((t) => t.id === s.activeId) ?? null);
}
