import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { DiscardedFile } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { useWorkdirStore } from "./store";
import styles from "./workdir.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

/** How long "Undo" stays on screen after a discard. */
export const UNDO_SECONDS = 10;

const MAX_LISTED = 8;

function FileLoss({ file }: { file: DiscardedFile }) {
  const { t } = useTranslation();
  const detail = file.untracked
    ? file.fileCount !== null
      ? t("discard.fileCount", { count: file.fileCount })
      : file.lines !== null
        ? t("discard.lineCount", { count: file.lines })
        : t("files.binary")
    : file.added === null
      ? t("files.binary")
      : `+${file.added} −${file.removed ?? 0}`;
  return (
    <li className={styles.lossItem}>
      <span className={styles.lossPath}>{file.path}</span>
      <span className={styles.lossDetail}>{detail}</span>
    </li>
  );
}

/** Confirmation before any discard: exactly what is lost, and how it can
 * be recovered. */
export function DiscardDialog() {
  const { t } = useTranslation();
  const pending = useWorkdirStore((s) => s.pendingDiscard);
  const cancel = useWorkdirStore((s) => s.cancelDiscard);
  const confirm = useWorkdirStore((s) => s.confirmDiscard);
  if (!pending) return null;

  const tracked = pending.files.filter((f) => !f.untracked);
  const untracked = pending.files.filter((f) => f.untracked);
  const added = tracked.reduce((n, f) => n + (f.added ?? 0), 0);
  const removed = tracked.reduce((n, f) => n + (f.removed ?? 0), 0);
  const isSelection = pending.target.kind === "selection";

  return (
    <ConfirmDialog
      title={t(isSelection ? "discard.titleLines" : "discard.title")}
      confirmLabel={t("discard.confirm")}
      cancelLabel={t("discard.cancel")}
      danger
      onCancel={cancel}
      onConfirm={() => void confirm()}
    >
      {tracked.length > 0 && (
        <p>
          {isSelection
            ? t("discard.lines", {
                count: added + removed,
                path: tracked[0]?.path,
                added,
                removed,
              })
            : t("discard.tracked", { count: tracked.length, added, removed })}
        </p>
      )}
      {untracked.length > 0 && (
        <p>{t("discard.untracked", { count: untracked.length })}</p>
      )}
      <ul className={styles.lossList}>
        {pending.files.slice(0, MAX_LISTED).map((file) => (
          <FileLoss key={file.path} file={file} />
        ))}
        {pending.files.length > MAX_LISTED && (
          <li className={styles.lossMore}>
            {t("discard.more", { count: pending.files.length - MAX_LISTED })}
          </li>
        )}
      </ul>
      {tracked.length > 0 && (
        <p>{t("discard.backup", { seconds: UNDO_SECONDS })}</p>
      )}
      {pending.files.some((f) => f.undoUnsupported) && (
        <p className={styles.lossWarning}>{t("discard.undoUnsupported")}</p>
      )}
      {untracked.length > 0 && <p>{t("discard.trashRecovery")}</p>}
      {tracked.length > 0 && <p>{t("discard.stagedKept")}</p>}
    </ConfirmDialog>
  );
}

/** "Changes discarded — Undo", for a few seconds after each discard. */
export function UndoToast() {
  const { t } = useTranslation();
  const last = useWorkdirStore((s) => s.lastDiscard);
  const undo = useWorkdirStore((s) => s.undoLastDiscard);
  const dismiss = useWorkdirStore((s) => s.dismissLastDiscard);
  const undoId = last?.undoId;
  const failed = last?.error !== null && last?.error !== undefined;

  useEffect(() => {
    if (undoId === undefined || failed) return;
    const timer = window.setTimeout(dismiss, UNDO_SECONDS * 1000);
    return () => window.clearTimeout(timer);
  }, [undoId, failed, dismiss]);

  if (!last) return null;
  return (
    <div className={styles.toast} role={last.error ? undefined : "status"}>
      {last.error ? (
        <ErrorReport error={last.error} lead={t("discard.undoFailed")} />
      ) : (
        <p>
          {t("discard.done", { count: last.files.length })}
          {last.stashed && (
            <span className={styles.toastNote}> {t("discard.inStash")}</span>
          )}
        </p>
      )}
      {!last.error && (
        <button
          type="button"
          className={styles.toastUndo}
          onClick={() => void undo()}
        >
          {t("discard.undo")}
        </button>
      )}
      <button
        type="button"
        className={styles.toastClose}
        aria-label={t("errors.dismiss")}
        onClick={dismiss}
      >
        ×
      </button>
    </div>
  );
}
