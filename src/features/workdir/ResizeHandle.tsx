import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import styles from "./workdir.module.css";

const STEP = 16;

/**
 * Horizontal divider that sets the height of the area below it (the commit
 * box), by dragging or with the keyboard: ↑/↓ grow and shrink it, Home/End
 * go to the limits, Enter or a double click go back to the natural height.
 */
export function ResizeHandle({
  label,
  value,
  measure,
  min,
  max,
  onChange,
}: {
  label: string;
  /** Current height in px, `null` = natural. */
  value: number | null;
  /** Height actually shown now (for `null` and for aria). */
  measure: () => number;
  min: number;
  /** Largest height allowed right now (depends on the panel's height). */
  max: () => number;
  onChange: (height: number | null) => void;
}) {
  const drag = useRef<{ startY: number; startHeight: number } | null>(null);
  const clamp = (h: number) => Math.round(Math.max(min, Math.min(max(), h)));
  const shown = () => Math.max(min, measure());

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { startY: event.clientY, startHeight: shown() };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    onChange(clamp(d.startHeight + (d.startY - event.clientY)));
  };
  const onPointerUp = () => {
    drag.current = null;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const current = value ?? shown();
    const next =
      event.key === "ArrowUp"
        ? clamp(current + STEP)
        : event.key === "ArrowDown"
          ? clamp(current - STEP)
          : event.key === "Home"
            ? clamp(max())
            : event.key === "End"
              ? min
              : event.key === "Enter"
                ? null
                : undefined;
    if (next === undefined) return;
    event.preventDefault();
    onChange(next);
  };

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={Math.max(min, Math.round(max()))}
      aria-valuenow={Math.round(value ?? shown())}
      tabIndex={0}
      className={styles.resizeHandle}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={() => onChange(null)}
      onKeyDown={onKeyDown}
    />
  );
}
