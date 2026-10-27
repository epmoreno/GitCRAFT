import { useTranslation } from "react-i18next";
import { useActiveRepository, useRepoStore } from "../features/repo/store";
import { Tooltip } from "../ui/Tooltip";
import styles from "./App.module.css";

const ACTIONS = ["pull", "push", "branch", "stash", "undo"] as const;

/**
 * Top bar. Doubles as the window title bar on macOS (overlay title bar),
 * so empty areas are drag regions.
 */
export function Toolbar() {
  const { t } = useTranslation();
  const active = useActiveRepository();
  const opening = useRepoStore((s) => s.opening);
  const pickAndOpen = useRepoStore((s) => s.pickAndOpen);

  // None of the repository actions exist yet; explain why they are disabled.
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
        {ACTIONS.map((action) => (
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
