import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type BranchNameCheck } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import dialog from "../../ui/ConfirmDialog.module.css";
import { useBranchStore } from "./store";
import styles from "./branches.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

/** How long typing pauses before the name is checked. */
const CHECK_DELAY_MS = 150;

/** Whichever branch dialog is open. */
export function BranchDialogs() {
  const current = useBranchStore((s) => s.dialog);
  if (!current) return null;
  switch (current.kind) {
    case "create":
    case "rename":
      return <NameDialog key={current.kind} />;
    case "delete":
      return <DeleteDialog />;
    case "blocked":
      return <BlockedDialog />;
  }
}

/** Name for a new branch, or a new name for one. Checked as it is typed. */
function NameDialog() {
  const { t } = useTranslation();
  const current = useBranchStore((s) => s.dialog);
  const close = useBranchStore((s) => s.closeDialog);
  const create = useBranchStore((s) => s.create);
  const rename = useBranchStore((s) => s.rename);
  const initial =
    current?.kind === "create" || current?.kind === "rename"
      ? current.name
      : "";
  const [name, setName] = useState(initial);
  const [checkout, setCheckout] = useState(true);
  const [check, setCheck] = useState<{
    name: string;
    result: BranchNameCheck;
  } | null>(null);
  const titleId = useId();
  const hintId = useId();
  const input = useRef<HTMLInputElement>(null);
  const repoId = current?.repoId ?? null;

  useEffect(() => {
    input.current?.select();
  }, []);

  useEffect(() => {
    if (repoId === null || name === "") return;
    let stale = false;
    const timer = window.setTimeout(() => {
      void commands.checkBranchName(repoId, name).then((result) => {
        if (!stale && result.status === "ok") {
          setCheck({ name, result: result.data });
        }
      });
    }, CHECK_DELAY_MS);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [repoId, name]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);

  if (current?.kind !== "create" && current?.kind !== "rename") return null;
  const isRename = current.kind === "rename";
  const unchanged = isRename && name === current.name;
  const verdict = check?.name === name ? check.result : null;
  const problem =
    name === "" || unchanged
      ? null
      : verdict === "invalid"
        ? t("branches.nameInvalid")
        : verdict === "exists"
          ? t("branches.nameExists", { name })
          : null;
  const ready = verdict === "valid" && !unchanged;

  return (
    <div className={dialog.backdrop}>
      <form
        className={dialog.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onSubmit={(event) => {
          event.preventDefault();
          if (name === "" || unchanged || problem) return;
          // Enter right after typing: check now instead of waiting.
          void (async () => {
            if (!ready) {
              const result = await commands.checkBranchName(
                current.repoId,
                name,
              );
              // If the check itself fails, the operation reports why.
              if (result.status === "ok") {
                setCheck({ name, result: result.data });
                if (result.data !== "valid") return;
              }
            }
            if (isRename) void rename(name);
            else void create(name, checkout);
          })();
        }}
      >
        <h2 id={titleId} className={dialog.title}>
          {isRename
            ? t("branches.renameTitle", { name: current.name })
            : t("branches.createTitle")}
        </h2>
        <div className={dialog.body}>
          {!isRename && (
            <p>
              {t(
                current.track
                  ? "branches.createTracking"
                  : "branches.createFrom",
                {
                  start: current.startLabel,
                },
              )}
            </p>
          )}
          <label className={styles.field}>
            <span>{t("branches.name")}</span>
            <input
              ref={input}
              className={styles.input}
              value={name}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              aria-invalid={problem !== null}
              aria-describedby={hintId}
              onChange={(event) => setName(event.target.value.trim())}
            />
          </label>
          <p id={hintId} className={problem ? styles.problem : styles.hint}>
            {problem ?? t("branches.nameHint")}
          </p>
          {!isRename && (
            <label className={styles.checkbox}>
              <input
                type="checkbox"
                checked={checkout}
                onChange={(event) => setCheckout(event.target.checked)}
              />
              {t("branches.switchToNew")}
            </label>
          )}
          {isRename && <p>{t("branches.renameNote")}</p>}
        </div>
        <div className={dialog.actions}>
          <button type="button" className={dialog.cancel} onClick={close}>
            {t("branches.cancel")}
          </button>
          <button
            type="submit"
            className={dialog.confirm}
            disabled={name === "" || unchanged || problem !== null}
          >
            {isRename ? t("branches.rename") : t("branches.create")}
          </button>
        </div>
      </form>
    </div>
  );
}

/** Deleting a branch: which commits would be left without one. */
function DeleteDialog() {
  const { t } = useTranslation();
  const current = useBranchStore((s) => s.dialog);
  const close = useBranchStore((s) => s.closeDialog);
  const confirm = useBranchStore((s) => s.confirmDelete);
  if (current?.kind !== "delete") return null;
  const { preview } = current;
  const more = preview.orphaned - preview.commits.length;

  return (
    <ConfirmDialog
      title={t("branches.deleteTitle", { name: preview.name })}
      confirmLabel={t("branches.delete")}
      cancelLabel={t("branches.cancel")}
      danger={preview.orphaned > 0}
      onCancel={close}
      onConfirm={() => void confirm()}
    >
      {preview.orphaned === 0 ? (
        <p>{t("branches.deleteSafe")}</p>
      ) : (
        <>
          <p>{t("branches.deleteOrphans", { count: preview.orphaned })}</p>
          <ul className={styles.commitList}>
            {preview.commits.map((commit) => (
              <li key={commit.oid} className={styles.commit}>
                <code className={styles.oid}>{commit.oid.slice(0, 7)}</code>
                <span className={styles.summary}>{commit.summary}</span>
                <span className={styles.author}>{commit.author}</span>
              </li>
            ))}
            {more > 0 && (
              <li className={styles.more}>
                {t("branches.moreCommits", { count: more })}
              </li>
            )}
          </ul>
        </>
      )}
      {preview.upstream && (
        <p>{t("branches.upstreamKept", { upstream: preview.upstream })}</p>
      )}
      <p>{t("branches.recreate")}</p>
      <pre className={`${styles.command} selectable`}>
        git branch {preview.name} {preview.tip}
      </pre>
    </ConfirmDialog>
  );
}

/** Uncommitted changes would be overwritten by the switch. */
function BlockedDialog() {
  const { t } = useTranslation();
  const current = useBranchStore((s) => s.dialog);
  const close = useBranchStore((s) => s.closeDialog);
  const stashAndSwitch = useBranchStore((s) => s.stashAndSwitch);
  if (current?.kind !== "blocked") return null;

  return (
    <ConfirmDialog
      title={t("branches.blockedTitle", { name: current.name })}
      confirmLabel={t("branches.stashAndSwitch")}
      cancelLabel={t("branches.cancel")}
      onCancel={close}
      onConfirm={() => void stashAndSwitch()}
    >
      <p>
        {t("branches.blocked", {
          count: current.files.length,
          name: current.name,
        })}
      </p>
      <ul className={styles.fileList}>
        {current.files.map((file) => (
          <li key={file.path} className={styles.file}>
            <span>{file.path}</span>
            {file.untracked && (
              <span className={styles.author}>{t("branches.untracked")}</span>
            )}
          </li>
        ))}
      </ul>
      <p>{t("branches.notForced")}</p>
      <p>
        {t("branches.stashExplained", {
          count: current.changed,
          name: current.name,
        })}
      </p>
    </ConfirmDialog>
  );
}

/** Result of the last branch operation that needs explaining. */
export function BranchToast() {
  const { t } = useTranslation();
  const notice = useBranchStore((s) => s.notice);
  const dismiss = useBranchStore((s) => s.dismissNotice);
  if (!notice) return null;

  return (
    <div
      className={styles.toast}
      role={notice.kind === "error" ? undefined : "status"}
    >
      <div className={styles.toastBody}>
        {notice.kind === "error" ? (
          <ErrorReport error={notice.error} lead={t("branches.failed")}>
            {notice.stashFor && (
              <p>{t("branches.failedAfterStash", { name: notice.stashFor })}</p>
            )}
          </ErrorReport>
        ) : notice.kind === "stashed" ? (
          <p>{t("branches.stashed", { name: notice.name })}</p>
        ) : (
          <>
            <p>{t("branches.deleted", { name: notice.name })}</p>
            <pre className="selectable">
              git branch {notice.name} {notice.tip}
            </pre>
          </>
        )}
      </div>
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
