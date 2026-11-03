import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import styles from "./settings.module.css";

export type ConfigRow = {
  key: string;
  before: string | null;
  after: string | null;
};

/**
 * Asks before writing git's configuration: the exact `git config` commands,
 * each with the value it replaces, and what they affect.
 */
export function ConfigConfirm({
  scope,
  rows,
  onConfirm,
  onCancel,
}: {
  scope: "global" | "local";
  rows: ConfigRow[];
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const flag = scope === "global" ? "--global" : "--local";
  return (
    <ConfirmDialog
      title={t(`settings.git.confirmTitle_${scope}`)}
      confirmLabel={t("settings.git.confirm")}
      cancelLabel={t("settings.git.cancel")}
      onCancel={onCancel}
      onConfirm={onConfirm}
    >
      <p>{t(`settings.git.confirmBody_${scope}`)}</p>
      <ul className={styles.changes}>
        {rows.map((row) => (
          <li key={row.key}>
            <code className="selectable">
              {row.after === null
                ? `git config ${flag} --unset-all ${row.key}`
                : `git config ${flag} ${row.key} ${row.after}`}
            </code>
            <span className={styles.hint}>
              {t("settings.git.before", {
                value: row.before ?? t("settings.git.notSet"),
              })}
            </span>
          </li>
        ))}
      </ul>
    </ConfirmDialog>
  );
}
