import { isShortcut } from "../../app/shortcuts";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { pickFolder, type SettingsLanguage, type TextSize } from "../../ipc";
import { ErrorReport } from "../../ui/ErrorReport";
import { GitDefaults } from "./GitDefaults";
import { GithubSection } from "./GithubSection";
import { IdentitySection } from "./IdentitySection";
import { ShortcutsSection } from "./ShortcutsSection";
import { useSettingsStore, type SettingsSection } from "./store";
import styles from "./settings.module.css";

/** Sections, in order; more are added as they exist. */
const SECTIONS: SettingsSection[] = [
  "general",
  "identity",
  "github",
  "shortcuts",
];

/** Settings window (gear in the toolbar, GitCRAFT → Settings…, ⌘,). */
export function SettingsDialog() {
  const { t } = useTranslation();
  const open = useSettingsStore((s) => s.open);
  const section = useSettingsStore((s) => s.section);
  const showSection = useSettingsStore((s) => s.showSection);
  const close = useSettingsStore((s) => s.close);
  const error = useSettingsStore((s) => s.error);
  const titleId = useId();
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    dialog.current?.querySelector<HTMLElement>("[aria-current]")?.focus();
    const onKey = (event: KeyboardEvent) => {
      // A confirmation open on top handles its own Escape.
      const modals = document.querySelectorAll('[aria-modal="true"]').length;
      if (isShortcut("escape", event) && modals <= 1) {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, close]);

  if (!open) return null;
  return (
    <div className={styles.backdrop}>
      <div
        ref={dialog}
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <nav className={styles.nav} aria-label={t("settings.sections")}>
          <h2 id={titleId} className={styles.title}>
            {t("settings.title")}
          </h2>
          {SECTIONS.map((s) => (
            <button
              key={s}
              type="button"
              className={styles.navItem}
              aria-current={s === section ? "page" : undefined}
              onClick={() => showSection(s)}
            >
              {t(`settings.section.${s}`)}
            </button>
          ))}
        </nav>
        <div className={styles.content}>
          {error && <ErrorReport error={error} lead={t("settings.failed")} />}
          {section === "general" && <GeneralSection />}
          {section === "identity" && <IdentitySection />}
          {section === "github" && <GithubSection />}
          {section === "shortcuts" && <ShortcutsSection />}
        </div>
        <button
          type="button"
          className={styles.close}
          aria-label={t("settings.close")}
          onClick={close}
        >
          ×
        </button>
      </div>
    </div>
  );
}

/** A labelled setting: the name, the control and an optional hint. */
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <div className={styles.field} role="group" aria-labelledby={id}>
      <div id={id} className={styles.fieldLabel}>
        {label}
      </div>
      <div className={styles.fieldControl}>{children}</div>
      {hint && <div className={styles.hint}>{hint}</div>}
    </div>
  );
}

const TEXT_SIZES: TextSize[] = ["small", "normal", "large", "larger"];

function GeneralSection() {
  const { t } = useTranslation();
  const general = useSettingsStore((s) => s.general);
  const update = useSettingsStore((s) => s.updateGeneral);
  return (
    <section aria-label={t("settings.section.general")}>
      <h3 className={styles.sectionTitle}>{t("settings.section.general")}</h3>
      <Field label={t("settings.language.label")}>
        <select
          className={styles.select}
          aria-label={t("settings.language.label")}
          value={general.language ?? "system"}
          onChange={(event) => {
            const value = event.target.value;
            void update({
              language: value === "system" ? null : (value as SettingsLanguage),
            });
          }}
        >
          <option value="system">{t("settings.language.system")}</option>
          <option value="es">Español</option>
          <option value="en">English</option>
        </select>
      </Field>
      <Field label={t("settings.textSize.label")}>
        <div className={styles.choices}>
          {TEXT_SIZES.map((size) => (
            <label key={size} className={styles.choice}>
              <input
                type="radio"
                name="text-size"
                checked={general.textSize === size}
                onChange={() => void update({ textSize: size })}
              />
              {t(`settings.textSize.${size}`)}
            </label>
          ))}
        </div>
      </Field>
      <Field
        label={t("settings.cloneFolder.label")}
        hint={t("settings.cloneFolder.hint")}
      >
        <span className={`${styles.path} selectable`}>
          {general.cloneFolder ?? t("settings.cloneFolder.lastUsed")}
        </span>
        <button
          type="button"
          className={styles.button}
          onClick={async () => {
            const folder = await pickFolder(t("settings.cloneFolder.pick"));
            if (folder !== null) void update({ cloneFolder: folder });
          }}
        >
          {t("settings.cloneFolder.choose")}
        </button>
        {general.cloneFolder !== null && (
          <button
            type="button"
            className={styles.button}
            onClick={() => void update({ cloneFolder: null })}
          >
            {t("settings.cloneFolder.useLast")}
          </button>
        )}
      </Field>
      <Field
        label={t("settings.autoFetch.label")}
        hint={t("settings.autoFetch.hint")}
      >
        <select
          className={styles.select}
          aria-label={t("settings.autoFetch.label")}
          value={general.autoFetchMinutes ?? 0}
          onChange={(event) => {
            const minutes = Number(event.target.value);
            void update({ autoFetchMinutes: minutes === 0 ? null : minutes });
          }}
        >
          <option value={0}>{t("settings.autoFetch.off")}</option>
          {AUTO_FETCH_MINUTES.map((m) => (
            <option key={m} value={m}>
              {t("settings.autoFetch.every", { count: m })}
            </option>
          ))}
        </select>
      </Field>
      <GitDefaults />
    </section>
  );
}

const AUTO_FETCH_MINUTES = [5, 10, 15, 30, 60];
