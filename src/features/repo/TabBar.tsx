import { useTranslation } from "react-i18next";
import { useRepoStore } from "./store";
import styles from "./repo.module.css";

export function TabBar() {
  const { t } = useTranslation();
  const tabs = useRepoStore((s) => s.tabs);
  const activeId = useRepoStore((s) => s.activeId);
  const activate = useRepoStore((s) => s.activate);
  const closeTab = useRepoStore((s) => s.closeTab);

  return (
    <div className={styles.tabbar} role="tablist">
      {tabs.length === 0 && (
        <span className={styles.tabbarEmpty}>{t("tabs.empty")}</span>
      )}
      {tabs.map(({ id, info }) => {
        const selected = id === activeId;
        return (
          <div
            key={id}
            className={
              selected ? `${styles.tab} ${styles.tabActive}` : styles.tab
            }
          >
            <button
              type="button"
              role="tab"
              aria-selected={selected}
              title={info.path}
              className={styles.tabLabel}
              onClick={() => activate(id)}
            >
              {info.name}
            </button>
            <button
              type="button"
              className={styles.tabClose}
              aria-label={t("tabs.close", { name: info.name })}
              onClick={() => void closeTab(id)}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
