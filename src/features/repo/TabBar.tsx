import { useRef, useState, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { useRepoStore } from "./store";
import styles from "./repo.module.css";

/** Pixels the pointer must move before a press becomes a drag. */
const DRAG_THRESHOLD = 5;

type Drag = { id: number; startX: number; moved: boolean };
type Menu = { x: number; y: number; label: string; items: MenuItem[] };

/** Open repositories. Drag a tab to reorder it; right-click for the same
 * (and closing) from the keyboard or without dragging. */
export function TabBar() {
  const { t } = useTranslation();
  const tabs = useRepoStore((s) => s.tabs);
  const activeId = useRepoStore((s) => s.activeId);
  const activate = useRepoStore((s) => s.activate);
  const closeTab = useRepoStore((s) => s.closeTab);
  const moveTab = useRepoStore((s) => s.moveTab);
  const elements = useRef(new Map<number, HTMLElement>());
  const drag = useRef<Drag | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

  /** Where the dragged tab goes: before the first tab whose middle is to
   * the right of the pointer. */
  const targetIndex = (id: number, x: number) => {
    let index = 0;
    for (const tab of tabs) {
      if (tab.id === id) continue;
      const rect = elements.current.get(tab.id)?.getBoundingClientRect();
      if (rect && x > rect.left + rect.width / 2) index += 1;
    }
    return index;
  };

  const onPointerDown = (event: PointerEvent, id: number) => {
    if (event.button !== 0) return;
    drag.current = { id, startX: event.clientX, moved: false };
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };

  const onPointerMove = (event: PointerEvent) => {
    const current = drag.current;
    if (!current) return;
    if (
      !current.moved &&
      Math.abs(event.clientX - current.startX) < DRAG_THRESHOLD
    ) {
      return;
    }
    current.moved = true;
    setDragging(current.id);
    moveTab(current.id, targetIndex(current.id, event.clientX));
  };

  const onPointerUp = () => {
    const current = drag.current;
    drag.current = null;
    setDragging(null);
    if (current && !current.moved) activate(current.id);
  };

  return (
    <div className={styles.tabbar} role="tablist" aria-label={t("tabs.label")}>
      {tabs.length === 0 && (
        <span className={styles.tabbarEmpty}>{t("tabs.empty")}</span>
      )}
      {tabs.map(({ id, info }, index) => {
        const selected = id === activeId;
        const items: MenuItem[] = [
          {
            label: t("tabs.moveLeft"),
            onSelect: () => moveTab(id, index - 1),
            disabled: index === 0,
            hint: t("tabs.alreadyFirst"),
          },
          {
            label: t("tabs.moveRight"),
            onSelect: () => moveTab(id, index + 1),
            disabled: index === tabs.length - 1,
            hint: t("tabs.alreadyLast"),
          },
          {
            label: t("tabs.closeTab"),
            onSelect: () => void closeTab(id),
          },
        ];
        return (
          <div
            key={id}
            ref={(element) => {
              if (element) elements.current.set(id, element);
              else elements.current.delete(id);
            }}
            className={[
              styles.tab,
              selected && styles.tabActive,
              dragging === id && styles.tabDragging,
            ]
              .filter(Boolean)
              .join(" ")}
            onPointerDown={(event) => onPointerDown(event, id)}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
            onContextMenu={(event) => {
              event.preventDefault();
              setMenu({
                x: event.clientX,
                y: event.clientY,
                label: info.name,
                items,
              });
            }}
          >
            <button
              type="button"
              role="tab"
              aria-selected={selected}
              title={
                index < 9
                  ? t("tabs.titleWithShortcut", {
                      path: info.path,
                      number: index + 1,
                    })
                  : info.path
              }
              className={styles.tabLabel}
              // Pointer presses activate on release (they may be drags).
              onClick={(event) => {
                if (event.detail === 0) activate(id);
              }}
            >
              {info.name}
            </button>
            <button
              type="button"
              className={styles.tabClose}
              aria-label={t("tabs.close", { name: info.name })}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={() => void closeTab(id)}
            >
              ×
            </button>
          </div>
        );
      })}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={menu.label}
          items={menu.items}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
