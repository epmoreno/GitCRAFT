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
};

export const initialWorkdirViewPrefs = {
  commitBoxHeight: null,
  collapsedSections: {},
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
    }),
    {
      name: "gitcraft.workdir-view",
      storage: createJSONStorage(() => localStorage),
    },
  ),
);
