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
  "general",
];

/** Every shortcut, from the same registry the key handlers use. Read-only
 * for now (customizing them is planned for phase 3). */
export function ShortcutsSection() {
  const { t } = useTranslation();
  const named = useKeyNames();
  return (
    <section aria-label={t("settings.section.shortcuts")}>
      <h3 className={styles.sectionTitle}>{t("settings.section.shortcuts")}</h3>
      <p className={styles.hint}>{t("settings.shortcuts.readOnly")}</p>
      {GROUPS.map((group) => (
        <table key={group} className={styles.shortcuts}>
          <caption>{t(`settings.shortcuts.group.${group}`)}</caption>
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
      ))}
    </section>
  );
}
