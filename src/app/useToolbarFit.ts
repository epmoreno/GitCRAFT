import { useLayoutEffect, useState, type RefObject } from "react";

/** What gives way in the toolbar when it does not fit, in this order. From
 * "publish" on, each one moves to the "…" menu. */
export const GIVING_WAY = [
  "brand",
  "branchName",
  "publish",
  "stash",
  "branch",
  "operations",
  // Only with large text in a narrow window.
  "undo",
] as const;

export type ToolbarPart = (typeof GIVING_WAY)[number];

/** The parts that move to the "…" menu. */
export const IN_MENU: readonly ToolbarPart[] = GIVING_WAY.slice(2);

/**
 * Keeps the toolbar inside the window: measures it (so it holds in any
 * language, font and text size) and drops parts until it fits. Which ones is
 * written to `data-dropped`, for the CSS, at once and without rendering;
 * returns them for the menu.
 */
export function useToolbarFit(
  bar: RefObject<HTMLElement | null>,
  middle: RefObject<HTMLElement | null>,
): readonly ToolbarPart[] {
  const [dropped, setDropped] = useState<readonly ToolbarPart[]>([]);

  useLayoutEffect(() => {
    const element = bar.current;
    if (!element) return;
    // One pixel of tolerance: with the text enlarged, sizes are fractions.
    const fits = () =>
      [element, middle.current].every(
        (e) => !e || e.scrollWidth <= e.clientWidth + 1,
      );
    const fit = () => {
      let count = 0;
      for (; count <= GIVING_WAY.length; count++) {
        const parts = GIVING_WAY.slice(0, count);
        element.dataset.dropped = [
          ...parts,
          ...(parts.some((p) => IN_MENU.includes(p)) ? ["menu"] : []),
        ].join(" ");
        if (fits()) break;
      }
      const parts = GIVING_WAY.slice(0, count);
      setDropped((old) => (old.length === parts.length ? old : parts));
    };
    fit();
    // Another size, or another content (branch, language, a running dot).
    const resize =
      typeof ResizeObserver === "undefined" ? null : new ResizeObserver(fit);
    resize?.observe(element);
    const content = new MutationObserver(fit);
    content.observe(element, {
      subtree: true,
      childList: true,
      characterData: true,
    });
    return () => {
      resize?.disconnect();
      content.disconnect();
    };
  }, [bar, middle]);

  return dropped;
}
