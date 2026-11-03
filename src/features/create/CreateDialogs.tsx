import { isShortcut } from "../../app/shortcuts";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  cloneRepository,
  commands,
  pickFolder,
  type CommandError,
  type Destination,
} from "../../ipc";
import { progressPercent } from "../graph/OptimizeNotice";
import { useRepoStore } from "../repo/store";
import dialog from "../../ui/ConfirmDialog.module.css";
import branches from "../branches/branches.module.css";
import styles from "./create.module.css";
import { useCreateStore } from "./store";
import { ErrorReport } from "../../ui/ErrorReport";

/** How long typing pauses before a name is checked. */
const CHECK_DELAY_MS = 150;

/** Whichever "new repository" dialog is open. */
export function CreateDialogs() {
  const current = useCreateStore((s) => s.dialog);
  if (!current) return null;
  return current.kind === "init" ? (
    <InitDialog key={current.path} />
  ) : (
    <CloneDialog />
  );
}

/** Closes on Escape, like every dialog. */
function useEscape(close: () => void) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isShortcut("escape", event)) {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);
}

/** `git init` in the chosen folder, with its first branch. */
function InitDialog() {
  const { t } = useTranslation();
  const current = useCreateStore((s) => s.dialog);
  const busy = useCreateStore((s) => s.busy);
  const error = useCreateStore((s) => s.error);
  const close = useCreateStore((s) => s.closeDialog);
  const confirm = useCreateStore((s) => s.confirmInit);
  const openInstead = useCreateStore((s) => s.openInstead);
  const [branch, setBranch] = useState(
    current?.kind === "init" ? current.check.defaultBranch : "main",
  );
  const [valid, setValid] = useState<{ name: string; ok: boolean } | null>(
    null,
  );
  const titleId = useId();
  const input = useRef<HTMLInputElement>(null);
  const path = current?.kind === "init" ? current.path : "";
  useEscape(close);

  useEffect(() => {
    input.current?.select();
  }, []);

  useEffect(() => {
    if (path === "" || branch === "") return;
    let stale = false;
    const timer = window.setTimeout(() => {
      void commands.validBranchName(path, branch).then((result) => {
        if (!stale && result.status === "ok") {
          setValid({ name: branch, ok: result.data });
        }
      });
    }, CHECK_DELAY_MS);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [path, branch]);

  if (current?.kind !== "init") return null;
  const { check } = current;
  const invalid = valid !== null && valid.name === branch && !valid.ok;

  if (check.alreadyRepository) {
    return (
      <div className={dialog.backdrop}>
        <div
          className={dialog.dialog}
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
        >
          <h2 id={titleId} className={dialog.title}>
            {t("init.alreadyTitle")}
          </h2>
          <div className={dialog.body}>
            <p className="selectable">{path}</p>
            <p>{t("init.already")}</p>
          </div>
          <div className={dialog.actions}>
            <button type="button" className={dialog.cancel} onClick={close}>
              {t("init.cancel")}
            </button>
            <button
              type="button"
              className={dialog.confirm}
              onClick={() => void openInstead()}
            >
              {t("init.open")}
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={dialog.backdrop}>
      <form
        className={dialog.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onSubmit={(event) => {
          event.preventDefault();
          if (branch !== "" && !invalid && !busy) void confirm(branch);
        }}
      >
        <h2 id={titleId} className={dialog.title}>
          {t("init.title")}
        </h2>
        <div className={dialog.body}>
          <p className="selectable">{path}</p>
          <label className={branches.field}>
            <span>{t("init.branch")}</span>
            <input
              ref={input}
              className={branches.input}
              value={branch}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              aria-invalid={invalid}
              onChange={(event) => setBranch(event.target.value.trim())}
            />
          </label>
          <p className={invalid ? branches.problem : branches.hint}>
            {invalid ? t("branches.nameInvalid") : t("init.branchHint")}
          </p>
          {check.inside && (
            <p className={branches.problem}>
              {t("init.nested", { parent: check.inside })}
            </p>
          )}
          <p>{t(check.empty ? "init.empty" : "init.notEmpty")}</p>
          {error && (
            <ErrorReport
              className={branches.problem}
              error={error}
              lead={t("init.failed")}
              path={path}
            />
          )}
        </div>
        <div className={dialog.actions}>
          <button type="button" className={dialog.cancel} onClick={close}>
            {t("init.cancel")}
          </button>
          <button
            type="submit"
            className={dialog.confirm}
            disabled={branch === "" || invalid || busy}
          >
            {t("init.create")}
          </button>
        </div>
      </form>
    </div>
  );
}

/** `git clone` of a URL into a new (or empty) folder, with progress. */
function CloneDialog() {
  const { t } = useTranslation();
  const current = useCreateStore((s) => s.dialog);
  const close = useCreateStore((s) => s.closeDialog);
  const adopt = useRepoStore((s) => s.adopt);
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState(
    current?.kind === "clone" ? current.parent : "",
  );
  const [name, setName] = useState("");
  /** The name follows the URL until the user edits it. */
  const [nameEdited, setNameEdited] = useState(false);
  const [destination, setDestination] = useState<{
    key: string;
    state: Destination | CommandError;
  } | null>(null);
  const [running, setRunning] = useState<{
    line: string;
    percent: number | null;
  } | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const titleId = useId();
  const parentId = useId();
  const input = useRef<HTMLInputElement>(null);
  useEscape(() => {
    if (!running) close();
  });

  useEffect(() => {
    input.current?.focus();
  }, []);

  useEffect(() => {
    if (nameEdited) return;
    let stale = false;
    void commands.defaultCloneName(url).then((result) => {
      if (!stale && result.status === "ok") setName(result.data);
    });
    return () => {
      stale = true;
    };
  }, [url, nameEdited]);

  const key = `${parent}\u0000${name}`;
  useEffect(() => {
    let stale = false;
    const timer = window.setTimeout(() => {
      void commands.checkCloneDestination(parent, name).then((result) => {
        if (!stale) {
          setDestination({
            key,
            state: result.status === "ok" ? result.data : result.error,
          });
        }
      });
    }, CHECK_DELAY_MS);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [parent, name, key]);

  if (current?.kind !== "clone") return null;
  const checked = destination?.key === key ? destination.state : null;
  const state = typeof checked === "string" ? checked : null;
  const checkError = typeof checked === "object" ? checked : null;
  const fullPath = parent && name ? `${parent.replace(/\/$/, "")}/${name}` : "";
  const ready =
    url.trim() !== "" && (state === "free" || state === "emptyFolder");

  const start = async () => {
    if (!ready || running) return;
    setError(null);
    setRunning({ line: "", percent: null });
    const result = await cloneRepository(url.trim(), parent, name, (line) =>
      setRunning((r) => ({
        line,
        percent: progressPercent(line) ?? r?.percent ?? null,
      })),
    );
    setRunning(null);
    if (result.status === "ok") {
      close();
      await adopt(result.data);
    } else if (result.error.kind !== "cancelled") {
      setError(result.error);
    }
  };

  return (
    <div className={dialog.backdrop}>
      <form
        className={dialog.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onSubmit={(event) => {
          event.preventDefault();
          void start();
        }}
      >
        <h2 id={titleId} className={dialog.title}>
          {t("clone.title")}
        </h2>
        <div className={dialog.body}>
          <label className={branches.field}>
            <span>{t("clone.url")}</span>
            <input
              ref={input}
              className={branches.input}
              value={url}
              placeholder={t("clone.urlPlaceholder")}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              disabled={running !== null}
              onChange={(event) => setUrl(event.target.value)}
            />
          </label>
          <p className={branches.hint}>{t("clone.urlHint")}</p>
          <div className={branches.field}>
            <label htmlFor={parentId}>{t("clone.parent")}</label>
            <span className={styles.row}>
              <input
                id={parentId}
                className={branches.input}
                value={parent}
                spellCheck={false}
                disabled={running !== null}
                onChange={(event) => setParent(event.target.value)}
              />
              <button
                type="button"
                className={dialog.cancel}
                disabled={running !== null}
                onClick={() =>
                  void pickFolder(t("clone.pickerTitle")).then((path) => {
                    if (path !== null) setParent(path);
                  })
                }
              >
                {t("clone.choose")}
              </button>
            </span>
          </div>
          <label className={branches.field}>
            <span>{t("clone.name")}</span>
            <input
              className={branches.input}
              value={name}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              disabled={running !== null}
              aria-invalid={state === "notEmpty" || state === "invalidName"}
              onChange={(event) => {
                setNameEdited(true);
                setName(event.target.value.trim());
              }}
            />
          </label>
          {checkError ? (
            <ErrorReport
              className={branches.problem}
              error={checkError}
              lead={t("clone.checkFailed")}
              path={fullPath}
            />
          ) : (
            <p
              className={
                state === "free" || state === "emptyFolder" || state === null
                  ? branches.hint
                  : branches.problem
              }
            >
              {state === null || !fullPath
                ? t("clone.destinationHint")
                : t(`clone.destination.${state}`, { path: fullPath })}
            </p>
          )}
          {running && (
            <div role="status" aria-live="polite">
              <p>{t("clone.running")}</p>
              <progress
                className={styles.bar}
                max={100}
                value={running.percent ?? undefined}
                aria-label={t("remote.progress")}
              />
              {running.line && <p className={styles.line}>{running.line}</p>}
            </div>
          )}
          {error && (
            <ErrorReport
              className={branches.problem}
              error={error}
              lead={t("clone.failed")}
              path={fullPath}
            />
          )}
        </div>
        <div className={dialog.actions}>
          {running ? (
            <button
              type="button"
              className={dialog.cancel}
              onClick={() => void commands.cancelClone()}
            >
              {t("clone.cancelRunning")}
            </button>
          ) : (
            <button type="button" className={dialog.cancel} onClick={close}>
              {t("init.cancel")}
            </button>
          )}
          <button
            type="submit"
            className={dialog.confirm}
            disabled={!ready || running !== null}
          >
            {t("clone.clone")}
          </button>
        </div>
      </form>
    </div>
  );
}
