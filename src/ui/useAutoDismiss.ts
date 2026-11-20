import { useEffect, useRef, useState, type FocusEvent } from "react";

/** Every notice closes by itself after this long on screen. */
export const NOTICE_SECONDS = 25;

/**
 * Closes `notice` after {@link NOTICE_SECONDS} on screen (`null`: it stays;
 * errors are passed as `null`, they stay until closed by hand). The time stops
 * while the pointer is over it or the keyboard focus is inside it, and
 * goes on from where it was when they leave. A new notice (another
 * object) starts again; so does one that comes back on screen (the tab of
 * its repository is active again).
 *
 * Returns the handlers for the notice's element.
 */
export function useAutoDismiss(notice: unknown, dismiss: () => void) {
  // Which notice the pointer and the focus are on: a new one starts
  // unpaused even if the pointer never left the old one.
  const [hovered, setHovered] = useState<unknown>(null);
  const [focused, setFocused] = useState<unknown>(null);
  const remaining = useRef(NOTICE_SECONDS * 1000);
  const latest = useRef(dismiss);
  useEffect(() => {
    latest.current = dismiss;
  });

  // Declared first: on a new notice it runs before the timer starts.
  useEffect(() => {
    remaining.current = NOTICE_SECONDS * 1000;
  }, [notice]);

  const paused = notice !== null && (hovered === notice || focused === notice);
  useEffect(() => {
    if (!notice || paused) return;
    const started = Date.now();
    const timer = window.setTimeout(() => latest.current(), remaining.current);
    return () => {
      window.clearTimeout(timer);
      remaining.current = Math.max(
        0,
        remaining.current - (Date.now() - started),
      );
    };
  }, [notice, paused]);

  return {
    onMouseEnter: () => setHovered(notice),
    onMouseLeave: () => setHovered(null),
    onFocus: () => setFocused(notice),
    onBlur: (event: FocusEvent<HTMLElement>) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
        setFocused(null);
      }
    },
  };
}
