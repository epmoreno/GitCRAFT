import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../../ipc";
import dialog from "../../ui/ConfirmDialog.module.css";
import branches from "../branches/branches.module.css";
import { useCreateStore } from "./store";

/** How long typing pauses before a name is checked. */
const CHECK_DELAY_MS = 150;

/** Whichever "new repository" dialog is open. */
export function CreateDialogs() {
  const current = useCreateStore((s) => s.dialog);
  if (!current) return null;
  return <InitDialog key={current.path} />;
}

/** Closes on Escape, like every dialog. */
function useEscape(close: () => void) {
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
  const [branch, setBranch] = useState(current?.check.defaultBranch ?? "main");
  const [valid, setValid] = useState<{ name: string; ok: boolean } | null>(
    null,
  );
  const titleId = useId();
  const input = useRef<HTMLInputElement>(null);
  const path = current?.path ?? "";
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
  const invalid = valid?.name === branch && !valid.ok;

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
            <p className={branches.problem} role="alert">
              {t(`errors.${error.kind}`, { path })}
            </p>
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
