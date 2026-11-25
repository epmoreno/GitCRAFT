import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isShortcut } from "../../app/shortcuts";
import { appVersion, commands } from "../../ipc";
import dialog from "../../ui/ConfirmDialog.module.css";
import styles from "./about.module.css";
import { useAboutStore } from "./store";

const REPOSITORY = "github.com/epmoreno/GitCRAFT";

/** "About GitCRAFT", the same on every system: each system's own panel
 * shows different things (Windows and Linux, not even the version). */
export function AboutDialog() {
  const open = useAboutStore((s) => s.isOpen);
  return open ? <About /> : null;
}

function About() {
  const { t } = useTranslation();
  const close = useAboutStore((s) => s.close);
  const [version, setVersion] = useState<string | null>(null);
  const titleId = useId();
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeButton.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (isShortcut("escape", event)) {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [close]);

  useEffect(() => {
    let stale = false;
    void appVersion().then((value) => {
      if (!stale) setVersion(value);
    });
    return () => {
      stale = true;
    };
  }, []);

  return (
    <div className={dialog.backdrop}>
      <div
        className={dialog.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <h2 id={titleId} className={dialog.title}>
          {t("about.title")}
        </h2>
        <div className={dialog.body}>
          <p className={styles.name}>GitCRAFT</p>
          <p>{t("app.description")}</p>
          <dl className={styles.facts}>
            <dt>{t("about.version")}</dt>
            <dd className="selectable">{version ?? "…"}</dd>
            <dt>{t("about.license")}</dt>
            <dd className="selectable">
              {t("about.mit")} · Copyright (c) 2026 Enrique
            </dd>
            <dt>{t("about.repository")}</dt>
            <dd>
              <button
                type="button"
                className={styles.link}
                title={t("about.openRepository")}
                onClick={() => void commands.openWebPage("project")}
              >
                {REPOSITORY}
              </button>
            </dd>
          </dl>
        </div>
        <div className={dialog.actions}>
          <button
            ref={closeButton}
            type="button"
            className={dialog.cancel}
            onClick={close}
          >
            {t("about.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
