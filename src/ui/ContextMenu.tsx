import { isShortcut } from "../app/shortcuts";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import styles from "./ContextMenu.module.css";

export type MenuItem = {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  /** Why it is disabled, shown as its tooltip. */
  hint?: string;
  danger?: boolean;
};

type Props = {
  /** Where it opens (viewport coordinates); kept inside the window. */
  x: number;
  y: number;
  label: string;
  items: MenuItem[];
  onClose: () => void;
};

/** A right-click menu. Arrow keys move, Enter picks, Escape or a click
 * elsewhere closes it. */
export function ContextMenu({ x, y, label, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const { width, height } = menu.getBoundingClientRect();
    setPosition({
      left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isShortcut("escape", event)) {
        event.stopPropagation();
        onClose();
      }
    };
    const onPointer = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  const move = (step: number) => {
    const buttons = [
      ...(ref.current?.querySelectorAll<HTMLButtonElement>(
        "button:not(:disabled)",
      ) ?? []),
    ];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(at + step + buttons.length) % buttons.length]?.focus();
  };

  return (
    <div
      ref={ref}
      className={styles.menu}
      role="menu"
      aria-label={label}
      style={position}
      onKeyDown={(event) => {
        if (isShortcut("menuNext", event)) move(1);
        else if (isShortcut("menuPrevious", event)) move(-1);
        else return;
        event.preventDefault();
      }}
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          role="menuitem"
          className={
            item.danger ? `${styles.item} ${styles.danger}` : styles.item
          }
          disabled={item.disabled}
          title={item.disabled ? item.hint : undefined}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
