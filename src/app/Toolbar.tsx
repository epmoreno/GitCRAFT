import { useTranslation } from "react-i18next";
import { useBranchStore } from "../features/branches/store";
import { useGraphStore } from "../features/graph/store";
import { useActiveRepository, useRepoStore } from "../features/repo/store";
import { Tooltip } from "../ui/Tooltip";
import styles from "./App.module.css";

const PENDING = ["pull", "push", "stash", "undo"] as const;

/**
 * Top bar. Doubles as the window title bar on macOS (overlay title bar),
 * so empty areas are drag regions.
 */
export function Toolbar() {
  const { t } = useTranslation();
  const active = useActiveRepository();
  const opening = useRepoStore((s) => s.opening);
  const pickAndOpen = useRepoStore((s) => s.pickAndOpen);
  const requestCreate = useBranchStore((s) => s.requestCreate);
  const head = useGraphStore((s) =>
    active ? (s.graphs[active.id]?.refs?.head ?? null) : null,
  );

  // The other actions do not exist yet; explain why they are disabled.
  const disabledReason = active
    ? t("toolbar.notYetAvailable")
    : t("toolbar.needsRepo");

  return (
    <header className={styles.toolbar} data-tauri-drag-region>
      <span className={styles.brand} data-tauri-drag-region>
        GitCRAFT
      </span>
      <button
        type="button"
        className={styles.primaryButton}
        disabled={opening}
        onClick={() => void pickAndOpen(t("start.dialogTitle"))}
      >
        {t("toolbar.openRepo")}
      </button>
      <div className={styles.toolbarActions} data-tauri-drag-region>
        <Tooltip
          label={
            !active
              ? t("toolbar.needsRepo")
              : head?.oid
                ? t("branches.newFromHead")
                : t("branches.needsCommit")
          }
        >
          <button
            type="button"
            className={styles.toolButton}
            disabled={!active || !head?.oid}
            onClick={() => {
              if (!active || !head?.oid) return;
              requestCreate(
                active.id,
                head.oid,
                head.type === "branch" ? head.name : head.oid.slice(0, 7),
              );
            }}
          >
            {t("toolbar.branch")}
          </button>
        </Tooltip>
        {PENDING.map((action) => (
          <Tooltip key={action} label={disabledReason}>
            <button type="button" className={styles.toolButton} disabled>
              {t(`toolbar.${action}`)}
            </button>
          </Tooltip>
        ))}
      </div>
    </header>
  );
}
