import { useTranslation } from "react-i18next";
import { useRepoStore } from "./store";
import styles from "./repo.module.css";

/** Explains why opening a folder failed, with git's raw output on demand. */
export function OpenErrorBanner() {
  const { t } = useTranslation();
  const openError = useRepoStore((s) => s.openError);
  const dismiss = useRepoStore((s) => s.dismissError);
  if (!openError) return null;

  const { path, error } = openError;
  return (
    <div className={styles.errorBanner} role="alert">
      <div className={styles.errorBody}>
        <strong className={styles.errorTitle}>{t("errors.openFailed")}</strong>
        <p className={styles.errorMessage}>
          {t(`errors.${error.kind}`, { path })}
        </p>
        {error.details && (
          <details className={styles.errorDetails}>
            <summary>{t("errors.showDetails")}</summary>
            <pre className="selectable">{error.details}</pre>
          </details>
        )}
      </div>
      <button type="button" className={styles.errorDismiss} onClick={dismiss}>
        {t("errors.dismiss")}
      </button>
    </div>
  );
}
