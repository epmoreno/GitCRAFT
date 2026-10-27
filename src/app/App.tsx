import { useTranslation } from "react-i18next";
import styles from "./App.module.css";
import { Toolbar } from "./Toolbar";

/** Three-zone shell: sidebar · commit graph · contextual detail panel. */
export function App() {
  const { t } = useTranslation();

  const sidebarSections = [
    t("sidebar.local"),
    t("sidebar.remote"),
    t("sidebar.tags"),
    t("sidebar.stashes"),
  ];

  return (
    <div className={styles.shell}>
      <Toolbar />
      <div className={styles.tabbar} role="tablist">
        <span className={styles.tabbarEmpty}>{t("tabs.empty")}</span>
      </div>
      <div className={styles.workspace}>
        <aside className={styles.sidebar} aria-label="sidebar">
          {sidebarSections.map((title) => (
            <section key={title} className={styles.sidebarSection}>
              <h2 className={styles.sidebarTitle}>{title}</h2>
              <p className={styles.sidebarEmpty}>{t("sidebar.empty")}</p>
            </section>
          ))}
        </aside>
        <main className={styles.graph} aria-label="graph">
          <div className={styles.emptyState}>
            <h1>{t("graph.emptyTitle")}</h1>
            <p>{t("graph.emptyHint")}</p>
          </div>
        </main>
        <aside className={styles.detail} aria-label="detail">
          <p className={styles.detailEmpty}>{t("detail.empty")}</p>
        </aside>
      </div>
    </div>
  );
}
