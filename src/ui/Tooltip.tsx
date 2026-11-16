import {
  cloneElement,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactElement,
} from "react";
import { createPortal } from "react-dom";
import styles from "./Tooltip.module.css";

type Props = {
  label: string;
  /** A single element; it is described by the tooltip for assistive tech. */
  children: ReactElement<{ "aria-describedby"?: string }>;
};

type Size = { width: number; height: number };
type Box = { left: number; top: number; right: number; bottom: number };

/** Gap between the tooltip and the window edge, and between it and its host. */
export const TOOLTIP_MARGIN = 8;
const GAP = 6;

export type TooltipPlacement = {
  /** Below the host unless there is no room there and more room above. */
  side: "below" | "above";
  /** Horizontal shift from centred on the host, to stay inside the window. */
  shift: number;
  /** Where it goes in the window (it is `position: fixed`). */
  left: number;
  top: number;
};

/** Where a tooltip of `tip` size goes so it stays inside `viewport`. */
export function placeTooltip(
  host: Box,
  tip: Size,
  viewport: Size,
): TooltipPlacement {
  const roomBelow = viewport.height - host.bottom - GAP - TOOLTIP_MARGIN;
  const roomAbove = host.top - GAP - TOOLTIP_MARGIN;
  const side =
    tip.height > roomBelow && roomAbove > roomBelow ? "above" : "below";

  const centre = (host.left + host.right) / 2;
  const left = centre - tip.width / 2;
  const maxLeft = viewport.width - TOOLTIP_MARGIN - tip.width;
  // Wider than the window: pin to the left margin.
  const clamped = Math.max(TOOLTIP_MARGIN, Math.min(left, maxLeft));
  return {
    side,
    shift: Math.round(clamped - left),
    left: Math.round(clamped),
    top: Math.round(
      side === "below" ? host.bottom + GAP : host.top - GAP - tip.height,
    ),
  };
}

/**
 * Hover/focus tooltip. Wraps the child in a span so it also works for
 * disabled buttons, which do not receive pointer events themselves.
 *
 * The tooltip itself lives in its own layer at the end of the page, so no
 * panel that clips its content (`overflow`) can cut it. It is placed each
 * time it is about to show, so it never leaves the window, and hides when
 * anything scrolls (it would no longer be next to its host).
 */
export function Tooltip({ label, children }: Props) {
  const id = useId();
  const hostRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const [placement, setPlacement] = useState<TooltipPlacement | null>(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const shown = hovered || focused;

  const place = () => {
    const host = hostRef.current;
    const tip = tipRef.current;
    if (!host || !tip) return;
    setPlacement(
      placeTooltip(
        host.getBoundingClientRect(),
        { width: tip.offsetWidth, height: tip.offsetHeight },
        { width: window.innerWidth, height: window.innerHeight },
      ),
    );
  };

  useEffect(() => {
    if (!shown) return;
    const hide = () => {
      setHovered(false);
      setFocused(false);
    };
    window.addEventListener("scroll", hide, true);
    window.addEventListener("resize", hide);
    return () => {
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("resize", hide);
    };
  }, [shown]);

  return (
    <span
      ref={hostRef}
      className={styles.host}
      onPointerEnter={() => {
        place();
        setHovered(true);
      }}
      onPointerLeave={() => setHovered(false)}
      onFocus={() => {
        place();
        setFocused(true);
      }}
      onBlur={() => setFocused(false)}
    >
      {cloneElement(children, { "aria-describedby": id })}
      {createPortal(
        <span
          ref={tipRef}
          role="tooltip"
          id={id}
          className={
            shown && placement ? `${styles.tip} ${styles.shown}` : styles.tip
          }
          style={
            placement
              ? { left: `${placement.left}px`, top: `${placement.top}px` }
              : undefined
          }
        >
          {label}
        </span>,
        document.body,
      )}
    </span>
  );
}
