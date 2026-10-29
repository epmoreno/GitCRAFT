import { useTranslation } from "react-i18next";
import branches from "../branches/branches.module.css";
import { useRemoteStore } from "./store";
import styles from "./remote.module.css";

/** Progress of the running fetch/pull/push, with "Cancel". */
export function RemoteProgress() {
  const { t } = useTranslation();
  const running = useRemoteStore((s) => s.running);
  const cancel = useRemoteStore((s) => s.cancel);
  if (!running) return null;
  return (
    <div className={styles.progress} role="status" aria-live="polite">
      <div className={styles.progressBody}>
        <p>
          {t(`remote.running.${running.kind}`, {
            remote: running.remote ?? t("remote.allRemotes"),
          })}
        </p>
        <progress
          className={styles.bar}
          max={100}
          value={running.percent ?? undefined}
          aria-label={t("remote.progress")}
        />
        {running.line && (
          <p className={styles.line} title={running.line}>
            {running.line}
          </p>
        )}
      </div>
      <button
        type="button"
        className={styles.cancel}
        onClick={() => void cancel()}
      >
        {t("remote.cancel")}
      </button>
    </div>
  );
}

/** Result of the last remote operation that needs explaining. */
export function RemoteToast() {
  const { t } = useTranslation();
  const notice = useRemoteStore((s) => s.notice);
  const dismiss = useRemoteStore((s) => s.dismissNotice);
  if (!notice) return null;
  return (
    <div
      className={branches.toast}
      role={notice.kind === "error" ? "alert" : "status"}
    >
      <div className={branches.toastBody}>
        {notice.kind === "error" ? (
          <>
            <p>
              {t(`remote.failed.${notice.operation}`)}{" "}
              {t(`errors.${notice.error.kind}`, { path: "" })}
            </p>
            {notice.error.details && (
              <pre className="selectable">{notice.error.details}</pre>
            )}
          </>
        ) : (
          <p>
            {t("remote.fetched", {
              remote: notice.remote ?? t("remote.allRemotes"),
            })}
          </p>
        )}
      </div>
      <button
        type="button"
        className={branches.toastClose}
        aria-label={t("errors.dismiss")}
        onClick={dismiss}
      >
        ×
      </button>
    </div>
  );
}
