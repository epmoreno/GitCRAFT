import { create } from "zustand";
import { useGraphStore } from "../graph/store";
import { useRemoteStore } from "../remote/store";
import { useStashStore } from "../stash/store";
import { forgetView } from "./viewState";
import { useWorkdirStore } from "../workdir/store";
import {
  commands,
  pickFolder,
  type CommandError,
  type FailedRestore,
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
  /**
   * A new tab showing the start page, always the last one. There is at most
   * one: ⌘T goes to it if it exists. It is active when `activeId` is null.
   * Opening a repository from it turns it into that repository's tab.
   */
  blankTab: boolean;
  recents: RecentRepository[];
  opening: boolean;
  openError: OpenError | null;
  /** The previous session has been restored (until then nothing is saved). */
  restored: boolean;
  /** Saved tabs that could not be reopened at start (shown once). */
  restoreFailures: FailedRestore[];
  /** The saved session itself could not be read. */
  restoreError: CommandError | null;

  /** Reopens the tabs of the previous session, with its active tab. */
  restoreSession: () => Promise<void>;
  dismissRestoreFailures: () => void;
  /** Moves tab `id` to position `index`. */
  moveTab: (id: number, index: number) => void;
  /** Activates the tab at `index` (0-based), if there is one. */
  activateIndex: (index: number) => void;
  /** Activates the next (`1`) or previous (`-1`) tab, wrapping around. */
  activateNext: (step: 1 | -1) => void;
  closeActiveTab: () => Promise<void>;
  /** Opens (or goes to) the new tab with the start page. */
  newTab: () => void;
  /** Shows the new tab, if there is one. */
  activateBlank: () => void;
  closeBlankTab: () => void;
  loadRecents: () => Promise<void>;
  openRepository: (path: string) => Promise<void>;
  /** Shows the folder picker and opens the chosen folder. */
  pickAndOpen: (dialogTitle: string) => Promise<void>;
  activate: (id: number) => void;
  /** Shows a repository opened elsewhere (e.g. just created) in a tab. */
  adopt: (opened: OpenedRepository) => Promise<void>;
  closeTab: (id: number) => Promise<void>;
  dismissError: () => void;
};

export const initialRepoState = {
  tabs: [],
  activeId: null,
  blankTab: false,
  recents: [],
  opening: false,
  openError: null,
  restored: false,
  restoreFailures: [],
  restoreError: null,
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
        // Appended where the new tab was (it is the last one).
        tabs: alreadyOpen ? s.tabs : [...s.tabs, opened],
        activeId: opened.id,
        opening: false,
        blankTab: s.blankTab && s.activeId !== null,
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

  adopt: async (opened) => {
    set((s) => ({
      tabs: s.tabs.some((t) => t.id === opened.id)
        ? s.tabs
        : [...s.tabs, opened],
      activeId: opened.id,
      openError: null,
      blankTab: s.blankTab && s.activeId !== null,
    }));
    await get().loadRecents();
  },

  restoreSession: async () => {
    const result = await commands.restoreSession();
    if (result.status === "ok") {
      const { opened, active, failed } = result.data;
      set((s) => ({
        // Anything opened meanwhile stays, after the restored tabs.
        tabs: [
          ...opened,
          ...s.tabs.filter((t) => !opened.some((o) => o.id === t.id)),
        ],
        activeId: s.activeId ?? active,
        restoreFailures: failed,
      }));
    } else {
      set({ restoreError: result.error });
    }
    set({ restored: true });
  },

  dismissRestoreFailures: () =>
    set({ restoreFailures: [], restoreError: null }),

  moveTab: (id, index) =>
    set((s) => {
      const from = s.tabs.findIndex((t) => t.id === id);
      const to = Math.max(0, Math.min(index, s.tabs.length - 1));
      if (from === -1 || from === to) return s;
      const tabs = [...s.tabs];
      const [tab] = tabs.splice(from, 1);
      if (tab) tabs.splice(to, 0, tab);
      return { tabs };
    }),

  activateIndex: (index) => {
    const { tabs, blankTab } = get();
    const tab = tabs[index];
    if (tab) set({ activeId: tab.id });
    else if (blankTab && index === tabs.length) set({ activeId: null });
  },

  activateNext: (step) => {
    const { tabs, activeId, blankTab } = get();
    // The new tab is the last position (id null).
    const order: (number | null)[] = [
      ...tabs.map((t) => t.id),
      ...(blankTab ? [null] : []),
    ];
    if (order.length === 0) return;
    const at = order.indexOf(activeId);
    const next = order[(at + step + order.length) % order.length];
    if (next !== undefined) set({ activeId: next });
  },

  closeActiveTab: async () => {
    const { activeId, blankTab } = get();
    if (activeId !== null) await get().closeTab(activeId);
    else if (blankTab) get().closeBlankTab();
  },

  newTab: () => set({ blankTab: true, activeId: null, openError: null }),

  activateBlank: () => {
    if (get().blankTab) set({ activeId: null });
  },

  closeBlankTab: () =>
    set((s) => ({
      blankTab: false,
      // It was the last tab: its left neighbour takes over.
      activeId:
        s.activeId === null
          ? (s.tabs[s.tabs.length - 1]?.id ?? null)
          : s.activeId,
    })),

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
    useWorkdirStore.getState().forget(id);
    useStashStore.getState().forget(id);
    useRemoteStore.getState().forget(id);
    forgetView(id);
    await commands.closeRepository(id);
  },

  dismissError: () => set({ openError: null }),
}));

export function useActiveRepository(): OpenedRepository | null {
  return useRepoStore((s) => s.tabs.find((t) => t.id === s.activeId) ?? null);
}

/** Saves the open tabs and the active one whenever they change (after the
 * previous session was restored, so a slow restore never erases it). */
useRepoStore.subscribe((state, previous) => {
  if (!state.restored) return;
  if (
    state.restored === previous.restored &&
    state.tabs === previous.tabs &&
    state.activeId === previous.activeId
  ) {
    return;
  }
  // The new tab is not saved; while it is active, its left neighbour is
  // what closing it would show.
  const active =
    state.tabs.find((t) => t.id === state.activeId) ??
    (state.blankTab ? state.tabs[state.tabs.length - 1] : undefined);
  void commands.saveSession(
    state.tabs.map((t) => t.info.path),
    active?.info.path ?? null,
  );
});
