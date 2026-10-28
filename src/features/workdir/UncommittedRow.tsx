import { useTranslation } from "react-i18next";
import { useGraphStore, WORKTREE } from "../graph/store";
import { changeCounts, useWorkdirStore } from "./store";
import styles from "./workdir.module.css";

/** "Uncommitted changes" entry at the top of the history. */
export function UncommittedRow({
  repoId,
  graphWidth,
}: {
  repoId: number;
  graphWidth: number;
}) {
  const { t } = useTranslation();
  const status = useWorkdirStore((s) => s.statuses[repoId]?.status ?? null);
  const selected = useGraphStore(
    (s) => s.graphs[repoId]?.selectedOid === WORKTREE,
  );
  const select = useGraphStore((s) => s.select);
  const counts = changeCounts(status);
  if (counts.total === 0) return null;

  return (
    <button
      type="button"
      className={selected ? `${styles.row} ${styles.rowSelected}` : styles.row}
      aria-pressed={selected}
      onClick={() => select(repoId, WORKTREE)}
    >
      <span className={styles.nodeArea} style={{ width: graphWidth }}>
        <span className={styles.node} aria-hidden="true" />
      </span>
      <span className={styles.label}>{t("workdir.row")}</span>
      {counts.conflicted > 0 && (
        <span className={`${styles.badge} ${styles.badgeConflict}`}>
          {t("workdir.conflictedCount", { count: counts.conflicted })}
        </span>
      )}
      {counts.staged > 0 && (
        <span className={`${styles.badge} ${styles.badgeStaged}`}>
          {t("workdir.stagedCount", { count: counts.staged })}
        </span>
      )}
      {counts.unstaged > 0 && (
        <span className={styles.badge}>
          {t("workdir.unstagedCount", { count: counts.unstaged })}
        </span>
      )}
    </button>
  );
}
