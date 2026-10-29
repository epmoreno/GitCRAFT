import { useLayoutEffect, useRef, type RefObject } from "react";
import { create } from "zustand";

/**
 * What each tab looks like beyond the stores' data: scroll positions and
 * collapsed sidebar sections. Kept per repository so switching tabs brings
 * each one back as it was.
 */

/** Scroll offsets by key (`<repoId>:<place>`); not reactive on purpose. */
const scrolls = new Map<string, number>();

/** How long a restored position waits for content that loads later. */
const RESTORE_WINDOW_MS = 2000;

/** Keeps `ref`'s vertical scroll under `key` and restores it on mount (or
 * when the key changes), also once content that loads afterwards (a diff,
 * a commit's details) is tall enough. Any user scroll wins. */
export function useRememberedScroll(
  ref: RefObject<HTMLElement | null>,
  key: string | null,
) {
  const current = useRef(key);
  useLayoutEffect(() => {
    current.current = key;
    const element = ref.current;
    if (!element || key === null) return;
    let pending: number | null = scrolls.get(key) ?? 0;
    const settle = () => {
      if (pending === null) return;
      element.scrollTop = pending;
      if (Math.abs(element.scrollTop - pending) < 1) pending = null;
    };
    settle();
    const stop = () => {
      pending = null;
    };
    const timer = window.setTimeout(stop, RESTORE_WINDOW_MS);
    const observer =
      typeof MutationObserver === "undefined"
        ? null
        : new MutationObserver(settle);
    observer?.observe(element, { childList: true, subtree: true });
    const onScroll = () => {
      // Positions clamped while content is still loading are not saved.
      if (pending === null && current.current !== null) {
        scrolls.set(current.current, element.scrollTop);
      }
    };
    const events = ["wheel", "pointerdown", "keydown", "touchstart"] as const;
    for (const name of events) element.addEventListener(name, stop);
    element.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      window.clearTimeout(timer);
      observer?.disconnect();
      for (const name of events) element.removeEventListener(name, stop);
      element.removeEventListener("scroll", onScroll);
    };
  }, [ref, key]);
}

type ViewState = {
  /** Collapsed sidebar sections, by `<repoId>:<section>`. */
  collapsed: Record<string, true>;
  toggleSection: (key: string) => void;
};

export const useViewStore = create<ViewState>()((set) => ({
  collapsed: {},
  toggleSection: (key) =>
    set((s) => {
      const collapsed = { ...s.collapsed };
      if (collapsed[key]) delete collapsed[key];
      else collapsed[key] = true;
      return { collapsed };
    }),
}));

/** Drops what a closed tab looked like. */
export function forgetView(repoId: number) {
  const prefix = `${repoId}:`;
  for (const key of [...scrolls.keys()]) {
    if (key.startsWith(prefix)) scrolls.delete(key);
  }
  useViewStore.setState((s) => ({
    collapsed: Object.fromEntries(
      Object.entries(s.collapsed).filter(([key]) => !key.startsWith(prefix)),
    ) as Record<string, true>,
  }));
}
