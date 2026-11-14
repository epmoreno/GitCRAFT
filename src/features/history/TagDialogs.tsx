import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type TagNameCheck } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { useGraphStore } from "../graph/store";
import { useTagStore } from "./tags";
import styles from "./history.module.css";

/** How long to wait after typing before checking the name. */
const CHECK_DELAY_MS = 200;

function CreateTagDialog({
  repoId,
  short,
  subject,
}: {
  repoId: number;
  short: string;
  subject: string;
}) {
  const { t } = useTranslation();
  const close = useTagStore((s) => s.closeDialog);
  const create = useTagStore((s) => s.create);
  const busy = useTagStore((s) => s.busy);
  const remotes = useGraphStore((s) => s.graphs[repoId]?.refs?.remotes ?? []);
  const [name, setName] = useState("");
  const [annotated, setAnnotated] = useState(true);
  const [message, setMessage] = useState("");
  const [push, setPush] = useState(false);
  const [remote, setRemote] = useState(
    remotes.includes("origin") ? "origin" : (remotes[0] ?? ""),
  );
  const [check, setCheck] = useState<{
    name: string;
    result: TagNameCheck;
  } | null>(null);

  useEffect(() => {
    if (name === "") return;
    let stale = false;
    const timer = setTimeout(() => {
      void commands.checkTagName(repoId, name).then((result) => {
        if (!stale && result.status === "ok")
          setCheck({ name, result: result.data });
      });
    }, CHECK_DELAY_MS);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [repoId, name]);

  const checked = check?.name === name ? check.result : null;
  const ready =
    checked === "valid" &&
    (!annotated || message.trim() !== "") &&
    (!push || remote !== "") &&
    !busy;
  return (
    <ConfirmDialog
      title={t("tags.create.title", { short, subject })}
      confirmLabel={t("tags.create.act")}
      cancelLabel={t("history.cancel")}
      confirmDisabled={!ready}
      onConfirm={() =>
        void create({
          name,
          message: annotated ? message : null,
          pushTo: push ? remote : null,
        })
      }
      onCancel={close}
    >
      <label className={styles.field}>
        <span>{t("tags.create.name")}</span>
        <input
          className={styles.input}
          value={name}
          spellCheck={false}
          autoCapitalize="off"
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      {name !== "" && (checked === "invalid" || checked === "exists") && (
        <p className={styles.fieldError} role="alert">
          {t(`tags.create.${checked}`)}
        </p>
      )}
      <fieldset className={styles.modes}>
        <legend>{t("tags.create.type")}</legend>
        <label className={styles.mode}>
          <input
            type="radio"
            name="tag-type"
            checked={annotated}
            onChange={() => setAnnotated(true)}
          />
          <span>
            <strong>{t("tags.create.annotated")}</strong>{" "}
            {t("tags.create.annotatedExplained")}
          </span>
        </label>
        <label className={styles.mode}>
          <input
            type="radio"
            name="tag-type"
            checked={!annotated}
            onChange={() => setAnnotated(false)}
          />
          <span>
            <strong>{t("tags.create.lightweight")}</strong>{" "}
            {t("tags.create.lightweightExplained")}
          </span>
        </label>
      </fieldset>
      {annotated && (
        <label className={styles.field}>
          <span>{t("tags.create.message")}</span>
          <textarea
            className={styles.input}
            rows={3}
            value={message}
            onChange={(e) => setMessage(e.target.value)}
          />
        </label>
      )}
      {remotes.length > 0 && (
        <div className={styles.pushRow}>
          <label className={styles.mode}>
            <input
              type="checkbox"
              checked={push}
              onChange={(e) => setPush(e.target.checked)}
            />
            {remotes.length === 1
              ? t("tags.create.pushTo", { remote: remotes[0] })
              : t("tags.create.pushToMany")}
          </label>
          {remotes.length > 1 && (
            <select
              className={styles.select}
              aria-label={t("tags.create.remote")}
              value={remote}
              disabled={!push}
              onChange={(e) => setRemote(e.target.value)}
            >
              {remotes.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          )}
        </div>
      )}
    </ConfirmDialog>
  );
}

export function TagDialogs() {
  const { t } = useTranslation();
  const dialog = useTagStore((s) => s.dialog);
  const close = useTagStore((s) => s.closeDialog);
  const confirmDelete = useTagStore((s) => s.confirmDelete);
  const confirmDeleteRemote = useTagStore((s) => s.confirmDeleteRemote);
  if (!dialog) return null;
  if (dialog.kind === "create") {
    return (
      <CreateTagDialog
        key={dialog.oid}
        repoId={dialog.repoId}
        short={dialog.short}
        subject={dialog.subject}
      />
    );
  }
  if (dialog.kind === "delete") {
    const { info, remotes } = dialog;
    const same = remotes?.filter((r) => r.same) ?? [];
    const different = remotes?.filter((r) => !r.same) ?? [];
    return (
      <ConfirmDialog
        title={t("tags.delete.title", { name: info.name })}
        confirmLabel={t("tags.delete.act")}
        cancelLabel={t("history.cancel")}
        danger
        onConfirm={() => void confirmDelete()}
        onCancel={close}
      >
        <p>{t("tags.delete.explained", { short: info.commit.slice(0, 7) })}</p>
        {remotes === null ? (
          <p className={styles.note}>{t("tags.delete.checking")}</p>
        ) : (
          <>
            {same.length > 0 && (
              <p>
                {t("tags.delete.alsoOn", {
                  remotes: same.map((r) => r.remote).join(", "),
                })}
              </p>
            )}
            {different.length > 0 && (
              <p>
                {t("tags.delete.differentOn", {
                  remotes: different.map((r) => r.remote).join(", "),
                })}
              </p>
            )}
            {dialog.remotesError && (
              <ErrorReport
                error={dialog.remotesError}
                lead={t("tags.delete.checkFailed")}
              />
            )}
          </>
        )}
      </ConfirmDialog>
    );
  }
  return (
    <ConfirmDialog
      title={t("tags.deleteRemote.title", {
        name: dialog.name,
        remote: dialog.tag.remote,
      })}
      confirmLabel={t("tags.deleteRemote.act", { remote: dialog.tag.remote })}
      cancelLabel={t("history.cancel")}
      danger
      onConfirm={() => void confirmDeleteRemote()}
      onCancel={close}
    >
      <p>
        {t("tags.deleteRemote.affectsOthers", { remote: dialog.tag.remote })}
      </p>
      <p>
        {t("tags.deleteRemote.lease", {
          remote: dialog.tag.remote,
          short: dialog.tag.oid.slice(0, 7),
        })}
      </p>
    </ConfirmDialog>
  );
}

export function TagToast({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const notice = useTagStore((s) =>
    s.notice && s.notice.repoId === repoId ? s.notice : null,
  );
  const dismiss = useTagStore((s) => s.dismissNotice);
  const requestDeleteRemote = useTagStore((s) => s.requestDeleteRemote);
  if (!notice || repoId === null) return null;
  const offer =
    notice.kind === "deleted" ? notice.stillOn.filter((r) => r.same) : [];
  return (
    <div
      className={styles.toast}
      role={notice.kind === "error" ? undefined : "status"}
    >
      <div className={styles.toastBody}>
        {notice.kind === "error" && (
          <ErrorReport error={notice.error} lead={t("history.failed")} />
        )}
        {notice.kind === "created" && (
          <p>
            {notice.pushedTo
              ? t("tags.notice.createdAndPushed", {
                  name: notice.name,
                  remote: notice.pushedTo,
                })
              : t("tags.notice.created", { name: notice.name })}
          </p>
        )}
        {notice.kind === "pushed" && (
          <p>
            {t("tags.notice.pushed", {
              name: notice.name,
              remote: notice.remote,
            })}
          </p>
        )}
        {notice.kind === "rejected" && (
          <>
            <p>
              {t("tags.notice.rejected", {
                name: notice.name,
                remote: notice.remote,
              })}
            </p>
            <pre className="selectable">{notice.detail}</pre>
          </>
        )}
        {notice.kind === "deleted" && (
          <p>
            {offer.length > 0
              ? t("tags.notice.deletedStillOn", {
                  name: notice.name,
                  remotes: offer.map((r) => r.remote).join(", "),
                })
              : t("tags.notice.deleted", { name: notice.name })}
          </p>
        )}
        {notice.kind === "deletedRemote" && (
          <p>
            {t("tags.notice.deletedRemote", {
              name: notice.name,
              remote: notice.remote,
            })}
          </p>
        )}
      </div>
      {notice.kind === "deleted" &&
        offer.map((tag) => (
          <button
            key={tag.remote}
            type="button"
            className={styles.toastAction}
            onClick={() => requestDeleteRemote(repoId, notice.name, tag)}
          >
            {t("tags.notice.deleteOn", { remote: tag.remote })}
          </button>
        ))}
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
