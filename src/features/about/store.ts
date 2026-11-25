import { create } from "zustand";

type AboutState = { isOpen: boolean; open: () => void; close: () => void };

export const useAboutStore = create<AboutState>()((set) => ({
  isOpen: false,
  open: () => set({ isOpen: true }),
  close: () => set({ isOpen: false }),
}));
