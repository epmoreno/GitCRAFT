import {
  cloneElement,
  useId,
  useRef,
  useState,
  type ReactElement,
} from "react";
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
  return { side, shift: Math.round(clamped - left) };
}

/**
 * Hover/focus tooltip. Wraps the child in a span so it also works for
 * disabled buttons, which do not receive pointer events themselves. It is
 * placed each time it is about to show, so it never leaves the window.
 */
export function Tooltip({ label, children }: Props) {
  const id = useId();
  const hostRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLSpanElement>(null);
  const [placement, setPlacement] = useState<TooltipPlacement>({
    side: "below",
    shift: 0,
  });
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
  return (
    <span
      ref={hostRef}
      className={styles.host}
      onPointerEnter={place}
      onFocus={place}
    >
      {cloneElement(children, { "aria-describedby": id })}
      <span
        ref={tipRef}
        role="tooltip"
        id={id}
        className={
          placement.side === "above"
            ? `${styles.tip} ${styles.above}`
            : styles.tip
        }
        style={{ "--tip-shift": `${placement.shift}px` } as React.CSSProperties}
      >
        {label}
      </span>
    </span>
  );
}
