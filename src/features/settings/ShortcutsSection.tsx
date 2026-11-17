import { useId } from "react";
import { useTranslation } from "react-i18next";
import {
  chordLabel,
  chordsOf,
  SHORTCUT_IDS,
  SHORTCUTS,
  type ShortcutGroup,
} from "../../app/shortcuts";
import { useKeyNames } from "../../app/useShortcutLabel";
import styles from "./settings.module.css";

const GROUPS: ShortcutGroup[] = [
  "app",
  "tabs",
  "changes",
  "commit",
  "diff",
  "conflicts",
  "history",
  "search",
  "fileHistory",
  "blame",
  "general",
];

/** Every shortcut, from the same registry the key handlers use. Read-only
 * for now (customizing them is planned for phase 3). */
export function ShortcutsSection() {
  const { t } = useTranslation();
  const named = useKeyNames();
  return (
    <section aria-label={t("settings.section.shortcuts")}>
      <p className={styles.hint}>{t("settings.shortcuts.readOnly")}</p>
      {GROUPS.map((group) => (
        <ShortcutGroupTable key={group} group={group} named={named} />
      ))}
    </section>
  );
}

/** One group: its title stays at the top while its rows scroll by. */
function ShortcutGroupTable({
  group,
  named,
}: {
  group: ShortcutGroup;
  named: (key: string) => string;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  return (
    <div className={styles.shortcutGroup}>
      <h4 id={titleId} className={styles.groupTitle}>
        {t(`settings.shortcuts.group.${group}`)}
      </h4>
      <table className={styles.shortcuts} aria-labelledby={titleId}>
        <tbody>
          {SHORTCUT_IDS.filter(
            // Only those that exist on this system.
            (id) => SHORTCUTS[id].group === group && chordsOf(id).length > 0,
          ).map((id) => (
            <tr key={id}>
              <th scope="row">{t(`settings.shortcuts.action.${id}`)}</th>
              <td>
                {chordsOf(id).map((chord, i) => (
                  <kbd key={i} className={styles.kbd}>
                    {chordLabel(chord, named)}
                  </kbd>
                ))}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
