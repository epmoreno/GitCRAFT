import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { PickPreview } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { useWorkdirStore } from "../workdir/store";
import { ResetDialog } from "./ResetDialog";
import { useHistoryStore } from "./store";
import styles from "./history.module.css";

/** At most this many files are listed in a dialog. */
const MAX_LISTED = 12;

export function Files({
  files,
}: {
  files: { path: string; untracked?: boolean }[];
}) {
  const { t } = useTranslation();
  return (
    <ul className={styles.files}>
      {files.slice(0, MAX_LISTED).map((f) => (
        <li key={f.path}>
          <code>{f.path}</code>
          {f.untracked && (
            <span className={styles.note}> {t("history.untracked")}</span>
          )}
        </li>
      ))}
      {files.length > MAX_LISTED && (
        <li className={styles.note}>
          {t("history.more", { count: files.length - MAX_LISTED })}
        </li>
      )}
    </ul>
  );
}

/** The cherry-pick (10.2) or revert (10.3) confirmation: clean, changes in
 * the way, conflicts, or both. */
function PickDialog({ preview }: { preview: PickPreview }) {
  const { t } = useTranslation();
  const close = useHistoryStore((s) => s.closeDialog);
  const confirm = useHistoryStore((s) => s.confirmPick);
  // This operation's own texts: history.pick.* or history.revert.*.
  const own = preview.kind === "revert" ? "history.revert" : "history.pick";
  const blocked = preview.blocked;
  const conflicts = preview.conflicts.length > 0;
  const values = {
    short: preview.commit.short,
    subject: preview.commit.subject,
    branch: preview.branch ?? "",
  };
  const button = blocked
    ? conflicts
      ? "saveAndActWithConflicts"
      : "saveAndAct"
    : conflicts
      ? "actWithConflicts"
      : "act";
  return (
    <ConfirmDialog
      title={t(`${own}.title`, values)}
      confirmLabel={t(`${own}.${button}`)}
      cancelLabel={t("history.cancel")}
      onConfirm={() => void confirm()}
      onCancel={close}
    >
      <p>{t(`${own}.explained`, values)}</p>
      {blocked && (
        <section aria-label={t("history.blockedTitle")}>
          <h3 className={styles.heading}>{t("history.blockedTitle")}</h3>
          <p>{t(`history.blocked.${blocked}`, { action: t(`${own}.noun`) })}</p>
          <Files files={preview.blocking} />
          <p>{t(`${own}.saveExplained`, values)}</p>
        </section>
      )}
      {conflicts && (
        <section aria-label={t("history.conflictsTitle")}>
          <h3 className={styles.heading}>
            {t(preview.exact ? `${own}.conflicts` : `${own}.mayConflict`, {
              count: preview.conflicts.length,
            })}
          </h3>
          <Files files={preview.conflicts.map((path) => ({ path }))} />
          <p>{t("history.conflictExplained")}</p>
          {blocked && <p>{t("history.abortKeepsSavedHere")}</p>}
        </section>
      )}
    </ConfirmDialog>
  );
}

export function HistoryDialogs() {
  const dialog = useHistoryStore((s) => s.dialog);
  if (dialog?.kind === "pick") {
    return <PickDialog preview={dialog.preview} />;
  }
  if (dialog?.kind === "reset") return <ResetDialog key={dialog.oid} />;
  return null;
}

/** The result of the last operation, and the changes saved in the stash
 * before it, offered back once nothing is in progress. */
export function HistoryToast({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const notice = useHistoryStore((s) =>
    s.notice && s.notice.repoId === repoId ? s.notice : null,
  );
  const saved = useHistoryStore((s) =>
    repoId === null ? undefined : s.saved[repoId],
  );
  const busy = useHistoryStore((s) => s.busy);
  const waiting = useWorkdirStore(
    (s) => repoId !== null && !!s.statuses[repoId]?.status?.inProgress,
  );
  const dismiss = useHistoryStore((s) => s.dismissNotice);
  const undo = useHistoryStore((s) => s.undo);
  const checkUndo = useHistoryStore((s) => s.checkUndo);
  // "Undo" lasts until the next write: check again whenever the status
  // changes.
  const version = useWorkdirStore((s) =>
    repoId === null ? 0 : (s.statuses[repoId]?.version ?? 0),
  );
  useEffect(() => {
    void checkUndo();
  }, [version, checkUndo]);
  const recover = useHistoryStore((s) => s.recoverSaved);
  const forget = useHistoryStore((s) => s.forgetSaved);
  const undoable =
    (notice?.kind === "reset" || notice?.kind === "merged") &&
    notice.undoId !== null &&
    !notice.undoGone;
  // While "Undo" is there it is the way back; the saved copy comes after.
  const offer = saved && !waiting && !undoable ? saved : null;
  if (repoId === null || (!notice && !offer)) return null;
  return (
    <div
      className={styles.toast}
      role={notice?.kind === "error" ? undefined : "status"}
    >
      <div className={styles.toastBody}>
        {notice?.kind === "error" && (
          <ErrorReport error={notice.error} lead={t("history.failed")} />
        )}
        {notice?.kind === "picked" && (
          <p>
            {t(
              notice.pick === "revert"
                ? "history.revert.done"
                : "history.pick.done",
              { subject: notice.subject, branch: notice.branch },
            )}
          </p>
        )}
        {notice?.kind === "reset" && (
          <p>
            {t("history.reset.done", {
              branch: notice.branch,
              short: notice.short,
            })}
            {notice.undoGone && ` ${t("history.reset.undoGone")}`}
          </p>
        )}
        {notice?.kind === "resetUndone" && (
          <p>{t("history.reset.undone", { branch: notice.branch })}</p>
        )}
        {notice?.kind === "merged" && (
          <p>
            {t(notice.fastForward ? "merge.doneFastForward" : "merge.done", {
              source: notice.source,
              target: notice.target,
            })}
            {notice.undoGone && ` ${t("history.reset.undoGone")}`}
          </p>
        )}
        {notice?.kind === "mergeUndone" && (
          <p>{t("merge.undone", { target: notice.target })}</p>
        )}
        {notice?.kind === "undoRefused" && (
          <p>
            {notice.op === "reset"
              ? t("history.reset.undoRefused", { short: notice.short })
              : t(
                  notice.stashed
                    ? "merge.undoRefusedStashed"
                    : "merge.undoRefused",
                  { source: notice.short },
                )}
          </p>
        )}
        {offer && (
          <p>
            {t(`history.savedNotice.${offer.pick}`, { short: offer.short })}
          </p>
        )}
      </div>
      {undoable && (
        <button
          type="button"
          className={styles.toastAction}
          disabled={busy}
          onClick={() => void undo()}
        >
          {t("history.reset.undo")}
        </button>
      )}
      {offer && (
        <button
          type="button"
          className={styles.toastAction}
          disabled={busy}
          onClick={() => void recover(repoId)}
        >
          {t("history.recover")}
        </button>
      )}
      <button
        type="button"
        className={styles.toastClose}
        aria-label={t("errors.dismiss")}
        onClick={() => {
          dismiss();
          if (offer) forget(repoId);
        }}
      >
        ×
      </button>
    </div>
  );
}
