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
}: {
  labels: RefLabel[] | undefined;
  color: string;
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
