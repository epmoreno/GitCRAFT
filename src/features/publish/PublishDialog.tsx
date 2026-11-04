import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type PublishInfo } from "../../ipc";
import { isShortcut } from "../../app/shortcuts";
import { ErrorReport } from "../../ui/ErrorReport";
import { useRemoteStore } from "../remote/store";
import { useSettingsStore } from "../settings/store";
import { usePublishStore } from "./store";
import styles from "./publish.module.css";

/** "1,2 MB" in the UI language. */
export function formatSize(bytes: number, language: string): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toLocaleString(language, { maximumFractionDigits: digits })} ${units[unit]}`;
}

/** Publish a repository without remotes on GitHub (personal account). */
export function PublishDialog() {
  const { t } = useTranslation();
  const dialog = usePublishStore((s) => s.dialog);
  const close = usePublishStore((s) => s.close);
  const titleId = useId();
  const busy =
    dialog?.step.kind === "creating" || dialog?.step.kind === "pushing";

  useEffect(() => {
    if (!dialog) return;
    const onKey = (event: KeyboardEvent) => {
      const modals = document.querySelectorAll('[aria-modal="true"]').length;
      if (isShortcut("escape", event) && modals <= 1 && !busy) {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dialog, busy, close]);

  if (!dialog) return null;
  return (
    <div className={styles.backdrop}>
      <div
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <h2 id={titleId} className={styles.title}>
          {t("publish.title")}
        </h2>
        {!dialog.info ? (
          dialog.error ? (
            <>
              <ErrorReport
                error={dialog.error}
                lead={t("publish.cannotPreview")}
              />
              <div className={styles.actions}>
                <button type="button" className={styles.button} onClick={close}>
                  {t("publish.close")}
                </button>
              </div>
            </>
          ) : (
            <p className={styles.muted}>{t("publish.loading")}</p>
          )
        ) : !dialog.info.account ? (
          <NoSession info={dialog.info} />
        ) : dialog.step.kind === "form" ? (
          <PublishForm info={dialog.info} />
        ) : (
          <Progress />
        )}
      </div>
    </div>
  );
}

function NoSession({ info }: { info: PublishInfo }) {
  const { t } = useTranslation();
  const close = usePublishStore((s) => s.close);
  const reload = usePublishStore((s) => s.reload);
  const openSettings = useSettingsStore((s) => s.openSettings);
  return (
    <>
      <p>{info.ghInstalled ? t("publish.noSession") : t("publish.noGh")}</p>
      <div className={styles.actions}>
        <button type="button" className={styles.button} onClick={close}>
          {t("publish.cancel")}
        </button>
        <button
          type="button"
          className={styles.button}
          onClick={() => void reload()}
        >
          {t("publish.checkAgain")}
        </button>
        <button
          type="button"
          className={styles.primary}
          onClick={() => {
            close();
            openSettings("github");
          }}
        >
          {t("publish.goToSettings")}
        </button>
      </div>
    </>
  );
}

function PublishForm({ info }: { info: PublishInfo }) {
  const { t, i18n } = useTranslation();
  const dialog = usePublishStore((s) => s.dialog);
  const publish = usePublishStore((s) => s.publish);
  const close = usePublishStore((s) => s.close);
  const [name, setName] = useState(info.suggestedName);
  const [description, setDescription] = useState("");
  const [isPrivate, setPrivate] = useState(true);
  const [nameOk, setNameOk] = useState(true);
  const nameId = useId();

  useEffect(() => {
    let stale = false;
    void commands.validRepoName(name.trim()).then((result) => {
      if (!stale) setNameOk(result.status === "ok" && result.data);
    });
    return () => {
      stale = true;
    };
  }, [name]);

  const { preview, account } = info;
  const warnings = [
    !preview.hasGitignore && t("publish.warn.noGitignore"),
    ...preview.largeFiles.map((f) =>
      t("publish.warn.largeFile", {
        path: f.path,
        size: formatSize(f.size ?? 0, i18n.language),
      }),
    ),
    ...preview.buildDirs.map((dir) =>
      t("publish.warn.buildDir", { path: dir }),
    ),
  ].filter((w): w is string => !!w);

  return (
    <form
      className={styles.form}
      onSubmit={(event) => {
        event.preventDefault();
        if (nameOk) void publish(name, description, isPrivate);
      }}
    >
      {dialog?.error && (
        <ErrorReport error={dialog.error} lead={t("publish.failed")} />
      )}
      <label className={styles.label} htmlFor={nameId}>
        {t("publish.name")}
      </label>
      <div className={styles.nameRow}>
        <span className={styles.owner}>@{account?.login} /</span>
        <input
          id={nameId}
          className={styles.input}
          value={name}
          aria-invalid={!nameOk}
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
      </div>
      {!nameOk && (
        <p className={styles.invalid}>{t("errors.invalidRepoName")}</p>
      )}
      <label className={styles.label}>
        {t("publish.description")}
        <textarea
          className={styles.textarea}
          value={description}
          rows={2}
          placeholder={t("publish.descriptionPlaceholder")}
          onChange={(event) => setDescription(event.target.value)}
        />
      </label>
      <fieldset className={styles.visibility}>
        <legend className={styles.label}>{t("publish.visibility")}</legend>
        <label className={styles.choice}>
          <input
            type="radio"
            name="visibility"
            checked={isPrivate}
            onChange={() => setPrivate(true)}
          />
          {t("publish.private")}
        </label>
        <label className={styles.choice}>
          <input
            type="radio"
            name="visibility"
            checked={!isPrivate}
            onChange={() => setPrivate(false)}
          />
          {t("publish.public")}
        </label>
      </fieldset>

      <section
        className={styles.summary}
        aria-label={t("publish.summaryTitle")}
      >
        <h3 className={styles.summaryTitle}>{t("publish.summaryTitle")}</h3>
        <p>
          {t("publish.summary", {
            branch: preview.branch,
            count: preview.commits,
            size: formatSize(preview.sizeBytes ?? 0, i18n.language),
          })}
        </p>
        {warnings.length > 0 && (
          <>
            <ul
              className={styles.warnings}
              aria-label={t("publish.warningsTitle")}
            >
              {warnings.map((w) => (
                <li key={w}>⚠ {w}</li>
              ))}
            </ul>
            <p className={styles.muted}>{t("publish.warningsHint")}</p>
          </>
        )}
      </section>

      <div className={styles.actions}>
        <button type="button" className={styles.button} onClick={close}>
          {t("publish.cancel")}
        </button>
        <button type="submit" className={styles.primary} disabled={!nameOk}>
          {isPrivate ? t("publish.submitPrivate") : t("publish.submitPublic")}
        </button>
      </div>
    </form>
  );
}

function Progress() {
  const { t } = useTranslation();
  const dialog = usePublishStore((s) => s.dialog);
  const close = usePublishStore((s) => s.close);
  const running = useRemoteStore((s) => s.running);
  const cancel = useRemoteStore((s) => s.cancel);
  if (!dialog) return null;
  const { step } = dialog;
  const branch = dialog.info?.preview.branch ?? "";
  const state = (n: 1 | 2): "done" | "active" =>
    n === 1
      ? step.kind === "creating"
        ? "active"
        : "done"
      : step.kind === "pushing"
        ? "active"
        : "done";
  return (
    <div className={styles.form}>
      <ol className={styles.steps}>
        <li data-state={state(1)}>{t("publish.step.create")}</li>
        {step.kind !== "creating" && (
          <li data-state={state(2)}>
            {t("publish.step.push", { branch })}
            {step.kind === "pushing" && running?.repoId === dialog.repoId && (
              <progress
                className={styles.bar}
                max={100}
                value={running.percent ?? undefined}
                aria-label={t("remote.progress")}
              />
            )}
          </li>
        )}
      </ol>
      {step.kind === "done" && (
        <p role="status">
          {t("publish.done")}{" "}
          <span className="selectable">{step.published.webUrl}</span>
        </p>
      )}
      {step.kind === "pushFailed" && (
        <div role="alert">
          <p>
            {t("publish.pushFailed", {
              name: step.published.fullName,
              url: step.published.remoteUrl,
            })}
          </p>
          {step.error && (
            <ErrorReport
              error={step.error}
              lead={t("publish.pushFailedLead")}
            />
          )}
        </div>
      )}
      <div className={styles.actions}>
        {step.kind === "pushing" ? (
          <button
            type="button"
            className={styles.button}
            onClick={() => void cancel()}
          >
            {t("publish.cancelPush")}
          </button>
        ) : step.kind !== "creating" ? (
          <button type="button" className={styles.primary} onClick={close}>
            {t("publish.close")}
          </button>
        ) : null}
      </div>
    </div>
  );
}
