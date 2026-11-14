import { useTranslation } from "react-i18next";
import { canDrop, useDragStore } from "./drag";
import styles from "./merge.module.css";

/** While a branch is dragged (task 8.4): its name follows the pointer, and
 * over a valid target, what dropping does. */
export function DragLayer() {
  const { t } = useTranslation();
  const dragging = useDragStore((s) => s.dragging);
  const over = useDragStore((s) => s.over);
  if (!dragging) return null;
  const valid = !!over && canDrop(dragging, dragging.repoId, over.name);
  return (
    <div
      className={styles.ghost}
      style={{ left: dragging.x + 14, top: dragging.y + 14 }}
      role="status"
      aria-label={t("drop.label", { name: dragging.branch.shortName })}
    >
      <span className={styles.ghostChip}>⑂ {dragging.branch.shortName}</span>
      {valid && (
        <span className={styles.ghostHint}>
          {t("drop.hint", { name: over.shortName })}
        </span>
      )}
    </div>
  );
}
