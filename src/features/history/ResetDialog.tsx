import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ResetMode, ResetPreview } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { useHistoryStore } from "./store";
import styles from "./history.module.css";

const MODES: ResetMode[] = ["soft", "mixed", "hard"];

/** At most this many files are listed per group. */
const MAX_FILES = 12;

function FileList({ files, note }: { files: string[]; note?: string }) {
  const { t } = useTranslation();
  return (
    <ul className={styles.files}>
      {files.slice(0, MAX_FILES).map((path) => (
        <li key={path}>
          <code>{path}</code>
          {note && <span className={styles.note}> {note}</span>}
        </li>
      ))}
      {files.length > MAX_FILES && (
        <li className={styles.note}>
          {t("history.more", { count: files.length - MAX_FILES })}
        </li>
      )}
    </ul>
  );
}

/** What the chosen mode would do, from the preview (task 10.4). */
function PreviewBody({ preview }: { preview: ResetPreview }) {
  const { t } = useTranslation();
  const onlyHere = preview.leaving.some((l) => l.onlyHere);
  const published = preview.leaving.filter((l) => l.published).length;
  const short = preview.target.short;
  return (
    <>
      <h3 className={styles.heading}>
        {t("history.reset.leaving", {
          count: preview.leavingTotal,
          branch: preview.branch ?? "",
        })}
      </h3>
      <ul className={styles.files}>
        {preview.leaving.map((l) => (
          <li key={l.commit.oid}>
            <code>{l.commit.short}</code> «{l.commit.subject}»
            {l.onlyHere && (
              <span className={styles.badgeDanger}>
                {" "}
                {t("history.reset.onlyHere")}
              </span>
            )}
            {l.published && (
              <span className={styles.note}>
                {" "}
                {t("history.reset.published")}
              </span>
            )}
          </li>
        ))}
        {preview.leavingTotal > preview.leaving.length && (
          <li className={styles.note}>
            {t("history.more", {
              count: preview.leavingTotal - preview.leaving.length,
            })}
          </li>
        )}
      </ul>
      {onlyHere && <p>{t("history.reset.onlyHereExplained")}</p>}
      {published > 0 && (
        <p className={styles.warning}>
          {t("history.reset.publishedWarning", {
            count: published,
            remotes: preview.publishedIn.join(", "),
          })}
        </p>
      )}
      {preview.mode === "mixed" && preview.unstagedByReset.length > 0 && (
        <>
          <h3 className={styles.heading}>{t("history.reset.unstagedTitle")}</h3>
          <FileList files={preview.unstagedByReset} />
          <p>{t("history.reset.copyInStash", { short })}</p>
        </>
      )}
      {preview.mode === "hard" && preview.lostChanges.length > 0 && (
        <>
          <h3 className={styles.heading}>{t("history.reset.lostTitle")}</h3>
          <FileList files={preview.lostChanges} />
          <p>{t("history.reset.copyInStash", { short })}</p>
        </>
      )}
      {preview.mode === "hard" && preview.overwrittenUntracked.length > 0 && (
        <>
          <h3 className={styles.heading}>{t("history.reset.trashTitle")}</h3>
          <FileList
            files={preview.overwrittenUntracked}
            note={t("history.untracked")}
          />
        </>
      )}
    </>
  );
}

export function ResetDialog() {
  const { t } = useTranslation();
  const dialog = useHistoryStore((s) =>
    s.dialog?.kind === "reset" ? s.dialog : null,
  );
  const busy = useHistoryStore((s) => s.busy);
  const close = useHistoryStore((s) => s.closeDialog);
  const setMode = useHistoryStore((s) => s.setResetMode);
  const confirm = useHistoryStore((s) => s.confirmReset);
  const [understood, setUnderstood] = useState(false);
  if (!dialog) return null;
  const { preview, mode } = dialog;
  const branch = preview?.branch ?? "";
  // Hard with something to lose asks for an explicit "I understand".
  const risky =
    !!preview &&
    mode === "hard" &&
    (preview.leaving.some((l) => l.onlyHere) || preview.lostChanges.length > 0);
  return (
    <ConfirmDialog
      title={t("history.reset.title", {
        branch,
        short: preview?.target.short ?? dialog.oid.slice(0, 7),
        subject: preview?.target.subject ?? "",
      })}
      confirmLabel={t("history.reset.act", { branch })}
      cancelLabel={t("history.cancel")}
      danger={mode === "hard"}
      confirmDisabled={!preview || busy || (risky && !understood)}
      onConfirm={() => void confirm()}
      onCancel={close}
    >
      {dialog.changed && (
        <p className={styles.warning} role="alert">
          {t("history.reset.changed")}
        </p>
      )}
      <fieldset className={styles.modes}>
        <legend>{t("history.reset.mode")}</legend>
        {MODES.map((m) => (
          <label key={m} className={styles.mode}>
            <input
              type="radio"
              name="reset-mode"
              value={m}
              checked={mode === m}
              onChange={() => {
                setUnderstood(false);
                void setMode(m);
              }}
            />
            <span>
              <strong>{t(`history.reset.modes.${m}.name`)}</strong>{" "}
              {t(`history.reset.modes.${m}.explained`)}
            </span>
          </label>
        ))}
      </fieldset>
      {!preview ? (
        <p className={styles.note}>{t("history.reset.loading")}</p>
      ) : (
        <PreviewBody preview={preview} />
      )}
      {risky && (
        <label className={styles.understand}>
          <input
            type="checkbox"
            checked={understood}
            onChange={(e) => setUnderstood(e.target.checked)}
          />
          {t("history.reset.understand", { branch })}
        </label>
      )}
    </ConfirmDialog>
  );
}
