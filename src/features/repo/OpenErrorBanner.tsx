import { useTranslation } from "react-i18next";
import { useRepoStore } from "./store";
import styles from "./repo.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

/** Explains why opening a folder failed, with git's raw output on demand. */
export function OpenErrorBanner() {
  const { t } = useTranslation();
  const openError = useRepoStore((s) => s.openError);
  const dismiss = useRepoStore((s) => s.dismissError);
  if (!openError) return null;

  const { path, error } = openError;
  return (
    <div className={styles.errorBanner}>
      <ErrorReport
        className={styles.errorBody}
        error={error}
        lead={t("errors.openFailed")}
        path={path}
      />
      <button type="button" className={styles.errorDismiss} onClick={dismiss}>
        {t("errors.dismiss")}
      </button>
    </div>
  );
}

/** Tabs of the previous session that could not be reopened (shown once:
 * they are no longer in the saved session). */
export function RestoreFailedBanner() {
  const { t } = useTranslation();
  const failures = useRepoStore((s) => s.restoreFailures);
  const restoreError = useRepoStore((s) => s.restoreError);
  const dismiss = useRepoStore((s) => s.dismissRestoreFailures);
  if (restoreError) {
    return (
      <div className={styles.errorBanner}>
        <ErrorReport
          className={styles.errorBody}
          error={restoreError}
          lead={t("tabs.restoreError")}
        />
        <button type="button" className={styles.errorDismiss} onClick={dismiss}>
          {t("errors.dismiss")}
        </button>
      </div>
    );
  }
  if (failures.length === 0) return null;
  return (
    <div className={styles.errorBanner}>
      <div className={styles.errorBody}>
        <strong className={styles.errorTitle}>
          {t("tabs.restoreFailed", { count: failures.length })}
        </strong>
        <ul className={styles.restoreList}>
          {failures.map(({ path, error }) => (
            <li key={path} className={styles.errorMessage}>
              <ErrorReport error={error} path={path} />
            </li>
          ))}
        </ul>
        <p className={styles.errorMessage}>{t("tabs.restoreForgotten")}</p>
      </div>
      <button type="button" className={styles.errorDismiss} onClick={dismiss}>
        {t("errors.dismiss")}
      </button>
    </div>
  );
}
