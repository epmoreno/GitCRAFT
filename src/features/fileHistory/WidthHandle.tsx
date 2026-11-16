import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { whichShortcut } from "../../app/shortcuts";
import styles from "./fileHistory.module.css";

const KEYS = [
  "paneWider",
  "paneNarrower",
  "paneWidest",
  "paneNarrowest",
  "paneReset",
] as const;

const STEP = 16;
const MIN = 200;

/**
 * Vertical divider that sets the width of the pane on its left (the list
 * of a file history), by dragging or with the keyboard: →/← widen and
 * narrow it, Home/End go to the limits, Enter or a double click go back to
 * the natural width.
 */
export function WidthHandle({
  label,
  value,
  measure,
  max,
  onChange,
}: {
  label: string;
  /** Current width in px, `null` = natural. */
  value: number | null;
  /** Width actually shown now. */
  measure: () => number;
  /** Largest width allowed right now. */
  max: () => number;
  onChange: (width: number | null) => void;
}) {
  const drag = useRef<{ startX: number; startWidth: number } | null>(null);
  const clamp = (w: number) =>
    Math.round(Math.max(MIN, Math.min(Math.max(MIN, max()), w)));

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { startX: event.clientX, startWidth: measure() };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    onChange(clamp(d.startWidth + (event.clientX - d.startX)));
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const action = whichShortcut(KEYS, event);
    if (!action) return;
    const current = value ?? measure();
    event.preventDefault();
    onChange(
      {
        paneWider: clamp(current + STEP),
        paneNarrower: clamp(current - STEP),
        paneWidest: clamp(max()),
        paneNarrowest: MIN,
        paneReset: null,
      }[action],
    );
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuemin={MIN}
      aria-valuemax={Math.max(MIN, Math.round(max()))}
      aria-valuenow={Math.round(value ?? measure())}
      tabIndex={0}
      className={styles.handle}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={() => onChange(null)}
      onKeyDown={onKeyDown}
    />
  );
}
