import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/**
 * How the uncommitted-changes panel is laid out. Remembered between
 * sessions in the webview's localStorage: losing it only resets the layout.
 */
export type WorkdirSection = "conflicts" | "unstaged" | "staged";

type WorkdirViewPrefs = {
  /** Height of the commit box in px; `null` = its natural height. */
  commitBoxHeight: number | null;
  setCommitBoxHeight: (height: number | null) => void;
  /** Collapsed sections (the same in every repository). */
  collapsedSections: Partial<Record<WorkdirSection, true>>;
  toggleSection: (section: WorkdirSection) => void;
  /** Files as a flat list of paths or as a folder tree. */
  layout: "list" | "tree";
  setLayout: (layout: "list" | "tree") => void;
  /**
   * Collapsed folders of the tree, by `<repoId>:<section>:<path>`. Only for
   * this session: not persisted.
   */
  collapsedDirs: Record<string, true>;
  toggleDir: (key: string) => void;
  setDirsCollapsed: (keys: string[], collapsed: boolean) => void;
};

export const initialWorkdirViewPrefs = {
  commitBoxHeight: null,
  collapsedSections: {},
  layout: "list" as const,
  collapsedDirs: {},
};

export const useWorkdirViewPrefs = create<WorkdirViewPrefs>()(
  persist(
    (set) => ({
      ...initialWorkdirViewPrefs,
      setCommitBoxHeight: (commitBoxHeight) => set({ commitBoxHeight }),
      toggleSection: (section) =>
        set((s) => {
          const collapsedSections = { ...s.collapsedSections };
          if (collapsedSections[section]) delete collapsedSections[section];
          else collapsedSections[section] = true;
          return { collapsedSections };
        }),
      setLayout: (layout) => set({ layout }),
      toggleDir: (key) =>
        set((s) => {
          const collapsedDirs = { ...s.collapsedDirs };
          if (collapsedDirs[key]) delete collapsedDirs[key];
          else collapsedDirs[key] = true;
          return { collapsedDirs };
        }),
      setDirsCollapsed: (keys, collapsed) =>
        set((s) => {
          const collapsedDirs = { ...s.collapsedDirs };
          for (const key of keys) {
            if (collapsed) collapsedDirs[key] = true;
            else delete collapsedDirs[key];
          }
          return { collapsedDirs };
        }),
    }),
    {
      name: "gitcraft.workdir-view",
      storage: createJSONStorage(() => localStorage),
      partialize: ({ commitBoxHeight, collapsedSections, layout }) => ({
        commitBoxHeight,
        collapsedSections,
        layout,
      }),
    },
  ),
);
