import { useTranslation } from "react-i18next";
import type { RefLabel } from "./refLabels";
import styles from "./graph.module.css";

const MAX_VISIBLE = 3;

const KIND_CLASS: Record<RefLabel["kind"], string | undefined> = {
  head: styles.chipHead,
  local: styles.chipLocal,
  remote: styles.chipRemote,
  tag: styles.chipTag,
};

/** Branch, tag and HEAD labels in front of a commit's summary. */
export function RefChips({
  labels,
  color,
  onBranchMenu,
}: {
  labels: RefLabel[] | undefined;
  color: string;
  /** Right click on a branch label: its own menu, not the commit's. */
  onBranchMenu?: (label: RefLabel, event: React.MouseEvent) => void;
}) {
  const { t } = useTranslation();
  if (!labels?.length) return null;
  const visible = labels.slice(0, MAX_VISIBLE);
  const hidden = labels.slice(MAX_VISIBLE);
  return (
    <span
      className={styles.chips}
      style={{ "--lane": color } as React.CSSProperties}
    >
      {visible.map((label) => (
        <span
          key={`${label.kind}:${label.text}`}
          className={[
            styles.chip,
            KIND_CLASS[label.kind],
            label.current ? styles.chipCurrent : undefined,
          ]
            .filter(Boolean)
            .join(" ")}
          title={t(`graph.refKind.${label.kind}`, { name: label.text })}
          onContextMenu={
            onBranchMenu && (label.kind === "local" || label.kind === "remote")
              ? (event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  onBranchMenu(label, event);
                }
              : undefined
          }
        >
          {label.text}
        </span>
      ))}
      {hidden.length > 0 && (
        <span
          className={`${styles.chip} ${styles.chipMore}`}
          title={hidden.map((l) => l.text).join("\n")}
        >
          {t("graph.moreRefs", { count: hidden.length })}
        </span>
      )}
    </span>
  );
}
