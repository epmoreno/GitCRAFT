import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import branches from "../branches/branches.module.css";
import { useWorkdirStore } from "../workdir/store";
import { useRemoteStore } from "./store";
import styles from "./remote.module.css";

const MAX_LISTED = 12;

function Files({ files }: { files: { path: string; note?: string }[] }) {
  const { t } = useTranslation();
  return (
    <ul className={branches.fileList}>
      {files.slice(0, MAX_LISTED).map(({ path, note }) => (
        <li key={path} className={branches.file}>
          <span>{path}</span>
          {note && <span className={branches.author}>{note}</span>}
        </li>
      ))}
      {files.length > MAX_LISTED && (
        <li className={branches.more}>
          {t("stash.moreFiles", { count: files.length - MAX_LISTED })}
        </li>
      )}
    </ul>
  );
}

/** Whichever remote dialog is open. */
export function RemoteDialogs() {
  const dialog = useRemoteStore((s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "pullBlocked":
      return <PullBlockedDialog />;
    case "pullConflict":
      return <PullConflictDialog />;
    case "abort":
      return <AbortDialog />;
  }
}

/** Uncommitted changes stop the pull: nothing was touched. */
function PullBlockedDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const saveLocalAndPull = useRemoteStore((s) => s.saveLocalAndPull);
  if (dialog?.kind !== "pullBlocked") return null;
  const files = dialog.files.map((f) => ({
    path: f.path,
    note: f.untracked ? t("branches.untracked") : undefined,
  }));

  if (dialog.reason === "unresolved") {
    return (
      <ConfirmDialog
        title={t("pull.blockedTitle")}
        confirmLabel={t("pull.ok")}
        cancelLabel={t("pull.cancel")}
        onCancel={close}
        onConfirm={close}
      >
        <p>{t("pull.unresolved")}</p>
        {files.length > 0 && <Files files={files} />}
      </ConfirmDialog>
    );
  }
  return (
    <ConfirmDialog
      title={t("pull.blockedTitle")}
      confirmLabel={t("pull.saveLocalAndPull")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={() => void saveLocalAndPull()}
    >
      <p>{t(`pull.blocked.${dialog.reason}`, { count: files.length })}</p>
      <Files files={files} />
      <p>{t("pull.notForced")}</p>
      <p>{t(`pull.alternatives.${dialog.reason}`)}</p>
    </ConfirmDialog>
  );
}

/** The pull would conflict: nothing was touched. */
function PullConflictDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const pullWithConflicts = useRemoteStore((s) => s.pullWithConflicts);
  const mode = useRemoteStore((s) =>
    dialog ? (s.pullModes[dialog.repoId]?.mode ?? "merge") : "merge",
  );
  if (dialog?.kind !== "pullConflict") return null;
  return (
    <ConfirmDialog
      title={t("pull.conflictTitle")}
      confirmLabel={t("pull.pullWithConflicts")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={() => void pullWithConflicts()}
    >
      <p>
        {t(dialog.exact ? "pull.conflicts" : "pull.mayConflict", {
          count: dialog.files.length,
          upstream: dialog.upstream,
        })}
      </p>
      <Files files={dialog.files.map((path) => ({ path }))} />
      <p>{t("pull.notForced")}</p>
      <p>{t(`pull.conflictExplained.${mode}`)}</p>
    </ConfirmDialog>
  );
}

/** Aborting the merge/rebase a pull left waiting. */
function AbortDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const confirm = useRemoteStore((s) => s.confirmAbort);
  if (dialog?.kind !== "abort") return null;
  return (
    <ConfirmDialog
      title={t(`pull.abortTitle.${dialog.operation}`)}
      confirmLabel={t("pull.abort")}
      cancelLabel={t("pull.cancel")}
      danger
      onCancel={close}
      onConfirm={() => void confirm()}
    >
      <p>{t("pull.abortExplained")}</p>
    </ConfirmDialog>
  );
}

/** A merge/rebase/cherry-pick/revert waiting for its conflicts. */
export function InProgressBanner({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const status = useWorkdirStore((s) =>
    repoId === null ? null : (s.statuses[repoId]?.status ?? null),
  );
  const requestAbort = useRemoteStore((s) => s.requestAbort);
  const operation = status?.inProgress ?? null;
  if (repoId === null || !operation) return null;
  const conflicts = status?.conflicted.length ?? 0;
  return (
    <div className={styles.banner} role="status">
      <p>
        {t(`pull.inProgress.${operation}`)}{" "}
        {conflicts > 0
          ? t("pull.inProgressConflicts", { count: conflicts })
          : t(`pull.inProgressNext.${operation}`)}
      </p>
      <button
        type="button"
        className={styles.bannerButton}
        onClick={() => requestAbort(repoId, operation)}
      >
        {t("pull.abortButton")}
      </button>
    </div>
  );
}
