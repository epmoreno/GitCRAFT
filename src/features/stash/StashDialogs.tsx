import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { StashEntry } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import dialog from "../../ui/ConfirmDialog.module.css";
import branches from "../branches/branches.module.css";
import { useWorkdirStore } from "../workdir/store";
import { storeCommand, useStashStore } from "./store";

const MAX_LISTED = 12;

/** Whichever stash dialog is open. */
export function StashDialogs() {
  const current = useStashStore((s) => s.dialog);
  if (!current) return null;
  switch (current.kind) {
    case "create":
      return <CreateDialog />;
    case "blocked":
      return <BlockedDialog />;
    case "conflict":
      return <ConflictDialog />;
    case "drop":
      return <DropDialog />;
  }
}

function Files({ paths }: { paths: { path: string; note?: string }[] }) {
  const { t } = useTranslation();
  return (
    <ul className={branches.fileList}>
      {paths.slice(0, MAX_LISTED).map(({ path, note }) => (
        <li key={path} className={branches.file}>
          <span>{path}</span>
          {note && <span className={branches.author}>{note}</span>}
        </li>
      ))}
      {paths.length > MAX_LISTED && (
        <li className={branches.more}>
          {t("stash.moreFiles", { count: paths.length - MAX_LISTED })}
        </li>
      )}
    </ul>
  );
}

/** "Stash" from the toolbar: optional message, untracked files or not. */
function CreateDialog() {
  const { t } = useTranslation();
  const current = useStashStore((s) => s.dialog);
  const close = useStashStore((s) => s.closeDialog);
  const create = useStashStore((s) => s.create);
  const repoId = current?.repoId ?? null;
  const status = useWorkdirStore((s) =>
    repoId === null ? null : (s.statuses[repoId]?.status ?? null),
  );
  const [message, setMessage] = useState("");
  const [untracked, setUntracked] = useState(false);
  const titleId = useId();
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    input.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);

  const tracked = new Set(
    [...(status?.staged ?? []), ...(status?.unstaged ?? [])].map((e) => e.path),
  ).size;
  const untrackedCount = status?.untracked.length ?? 0;
  const count = tracked + (untracked ? untrackedCount : 0);

  return (
    <div className={dialog.backdrop}>
      <form
        className={dialog.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onSubmit={(event) => {
          event.preventDefault();
          if (count > 0) void create(message, untracked);
        }}
      >
        <h2 id={titleId} className={dialog.title}>
          {t("stash.createTitle")}
        </h2>
        <div className={dialog.body}>
          <p>{t("stash.createExplained")}</p>
          <label className={branches.field}>
            <span>{t("stash.message")}</span>
            <input
              ref={input}
              className={branches.input}
              value={message}
              placeholder={t("stash.messagePlaceholder")}
              onChange={(event) => setMessage(event.target.value)}
            />
          </label>
          <p className={branches.hint}>{t("stash.messageHint")}</p>
          <label className={branches.checkbox}>
            <input
              type="checkbox"
              checked={untracked}
              disabled={untrackedCount === 0}
              onChange={(event) => setUntracked(event.target.checked)}
            />
            {t("stash.includeUntracked", { count: untrackedCount })}
          </label>
          <p>
            {count > 0
              ? t("stash.willSave", { count })
              : t("stash.nothingToSave")}
          </p>
        </div>
        <div className={dialog.actions}>
          <button type="button" className={dialog.cancel} onClick={close}>
            {t("stash.cancel")}
          </button>
          <button
            type="submit"
            className={dialog.confirm}
            disabled={count === 0}
          >
            {t("stash.create")}
          </button>
        </div>
      </form>
    </div>
  );
}

/** Applying would overwrite uncommitted changes: nothing was touched. */
function BlockedDialog() {
  const { t } = useTranslation();
  const current = useStashStore((s) => s.dialog);
  const close = useStashStore((s) => s.closeDialog);
  const saveLocalAndApply = useStashStore((s) => s.saveLocalAndApply);
  if (current?.kind !== "blocked") return null;
  return (
    <ConfirmDialog
      title={t("stash.blockedTitle")}
      confirmLabel={t("stash.saveLocalAndApply")}
      cancelLabel={t("stash.cancel")}
      onCancel={close}
      onConfirm={() => void saveLocalAndApply()}
    >
      <p>{t("stash.blocked", { count: current.files.length })}</p>
      <Files
        paths={current.files.map((f) => ({
          path: f.path,
          note: f.untracked ? t("branches.untracked") : undefined,
        }))}
      />
      <p>{t("stash.notForced")}</p>
      <p>{t("stash.saveLocalExplained")}</p>
    </ConfirmDialog>
  );
}

/** Applying would conflict with the current branch: nothing was touched. */
function ConflictDialog() {
  const { t } = useTranslation();
  const current = useStashStore((s) => s.dialog);
  const close = useStashStore((s) => s.closeDialog);
  const applyWithConflicts = useStashStore((s) => s.applyWithConflicts);
  if (current?.kind !== "conflict") return null;
  return (
    <ConfirmDialog
      title={t("stash.conflictTitle")}
      confirmLabel={t("stash.applyWithConflicts")}
      cancelLabel={t("stash.cancel")}
      onCancel={close}
      onConfirm={() => void applyWithConflicts()}
    >
      <p>
        {t(current.exact ? "stash.conflicts" : "stash.mayConflict", {
          count: current.files.length,
        })}
      </p>
      <Files paths={current.files.map((path) => ({ path }))} />
      <p>{t("stash.notForced")}</p>
      <p>{t("stash.conflictExplained")}</p>
      {current.mode.pop && <p>{t("stash.popKeeps")}</p>}
    </ConfirmDialog>
  );
}

/** Deleting an entry: always confirmed, with the way back. */
function DropDialog() {
  const { t } = useTranslation();
  const current = useStashStore((s) => s.dialog);
  const close = useStashStore((s) => s.closeDialog);
  const confirm = useStashStore((s) => s.confirmDrop);
  if (current?.kind !== "drop") return null;
  const { entry } = current;
  return (
    <ConfirmDialog
      title={t("stash.dropTitle")}
      confirmLabel={t("stash.drop")}
      cancelLabel={t("stash.cancel")}
      danger
      onCancel={close}
      onConfirm={() => void confirm()}
    >
      <p className={branches.summary}>«{entry.message}»</p>
      <p>
        {t(entry.untracked ? "stash.dropLosesUntracked" : "stash.dropLoses")}
      </p>
      <p>{t("stash.hash")}</p>
      <pre className={`${branches.command} selectable`}>{entry.oid}</pre>
      <p>{t("stash.recover")}</p>
      <pre className={`${branches.command} selectable`}>
        {storeCommand(entry)}
      </pre>
    </ConfirmDialog>
  );
}

function DroppedNotice({ entry }: { entry: StashEntry }) {
  const { t } = useTranslation();
  return (
    <>
      <p>{t("stash.dropped")}</p>
      <pre className="selectable">{storeCommand(entry)}</pre>
    </>
  );
}

/** Result of the last stash operation. */
export function StashToast() {
  const { t } = useTranslation();
  const notice = useStashStore((s) => s.notice);
  const dismiss = useStashStore((s) => s.dismissNotice);
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
              {t("stash.failed")}{" "}
              {t(`errors.${notice.error.kind}`, { path: "" })}
            </p>
            {notice.error.details && (
              <pre className="selectable">{notice.error.details}</pre>
            )}
          </>
        ) : notice.kind === "created" ? (
          <p>{t("stash.created")}</p>
        ) : notice.kind === "applied" ? (
          <p>
            {t(notice.dropped ? "stash.popped" : "stash.applied")}
            {notice.savedLocal && ` ${t("stash.localSaved")}`}
          </p>
        ) : notice.kind === "conflicted" ? (
          <>
            <p>{t("stash.conflicted", { count: notice.files.length })}</p>
            <pre className="selectable">{notice.files.join("\n")}</pre>
          </>
        ) : (
          <DroppedNotice entry={notice.entry} />
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
