import { useTranslation } from "react-i18next";
import type { RefLabel } from "./refLabels";
import { beginBranchDrag, dropTargetProps, useDropState } from "../merge/drag";
import dragStyles from "../merge/merge.module.css";
import styles from "./graph.module.css";

const MAX_VISIBLE = 3;
/** A label is never cut below this, or below its whole text if shorter. */
const MIN_WIDTH = 60;

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
  repoId,
  onBranchMenu,
  max = MAX_VISIBLE,
}: {
  labels: RefLabel[] | undefined;
  color: string;
  /** How many are shown; the rest are "+N". */
  max?: number;
  /** Branch labels can be dragged onto another branch (task 8.4). */
  repoId?: number;
  /** Right click on a branch label: its own menu, not the commit's. */
  onBranchMenu?: (label: RefLabel, event: React.MouseEvent) => void;
}) {
  const { t } = useTranslation();
  if (!labels?.length) return null;
  const visible = labels.slice(0, max);
  const hidden = labels.slice(max);
  return (
    <span
      className={styles.chips}
      style={{ "--lane": color } as React.CSSProperties}
    >
      {visible.map((label) => (
        <Chip
          key={`${label.kind}:${label.text}`}
          label={label}
          repoId={repoId}
          onBranchMenu={onBranchMenu}
        />
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

function Chip({
  label,
  repoId,
  onBranchMenu,
}: {
  label: RefLabel;
  repoId?: number;
  onBranchMenu?: (label: RefLabel, event: React.MouseEvent) => void;
}) {
  const { t } = useTranslation();
  const branch = label.kind === "local" || label.kind === "remote";
  const dropState = useDropState(repoId ?? null, label.refname);
  const named = { name: label.refname, shortName: label.text };
  return (
    <span
      className={[
        styles.chip,
        KIND_CLASS[label.kind],
        label.current ? styles.chipCurrent : undefined,
        dropState === "target" ? dragStyles.dropTargetChip : undefined,
        dropState === "over" ? dragStyles.dropOverChip : undefined,
      ]
        .filter(Boolean)
        .join(" ")}
      style={{
        minWidth: `min(${MIN_WIDTH}px, calc(${label.text.length}ch + 14px))`,
      }}
      title={t(`graph.refKind.${label.kind}`, { name: label.text })}
      // Every label says it is one, so the cursor shows where a branch
      // cannot be dropped (canDrop accepts local branches only).
      {...(repoId !== undefined && label.kind !== "head"
        ? dropTargetProps(repoId, named)
        : {})}
      onPointerDown={
        repoId !== undefined && branch
          ? (event) => beginBranchDrag(event, repoId, named)
          : undefined
      }
      onContextMenu={
        onBranchMenu && branch
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
  );
}
