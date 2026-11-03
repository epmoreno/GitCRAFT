import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/**
 * How the uncommitted-changes panel is laid out. Remembered between
 * sessions in the webview's localStorage: losing it only resets the layout.
 */
type WorkdirViewPrefs = {
  /** Height of the commit box in px; `null` = its natural height. */
  commitBoxHeight: number | null;
  setCommitBoxHeight: (height: number | null) => void;
};

export const initialWorkdirViewPrefs = { commitBoxHeight: null };

export const useWorkdirViewPrefs = create<WorkdirViewPrefs>()(
  persist(
    (set) => ({
      ...initialWorkdirViewPrefs,
      setCommitBoxHeight: (commitBoxHeight) => set({ commitBoxHeight }),
    }),
    {
      name: "gitcraft.workdir-view",
      storage: createJSONStorage(() => localStorage),
    },
  ),
);
