import { useTranslation } from "react-i18next";
import styles from "./App.module.css";

const ACTIONS = ["pull", "push", "branch", "stash", "undo"] as const;

/**
 * Top bar. Doubles as the window title bar on macOS (overlay title bar),
 * so empty areas are drag regions.
 */
export function Toolbar() {
  const { t } = useTranslation();

  return (
    <header className={styles.toolbar} data-tauri-drag-region>
      <span className={styles.brand} data-tauri-drag-region>
        GitCRAFT
      </span>
      <button type="button" className={styles.primaryButton}>
        {t("toolbar.openRepo")}
      </button>
      <div className={styles.toolbarActions} data-tauri-drag-region>
        {ACTIONS.map((action) => (
          <button
            key={action}
            type="button"
            className={styles.toolButton}
            disabled
          >
            {t(`toolbar.${action}`)}
          </button>
        ))}
      </div>
    </header>
  );
}
