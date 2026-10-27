import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { GraphView } from "../features/graph/GraphView";
import { OpenErrorBanner } from "../features/repo/OpenErrorBanner";
import { StartPage } from "../features/repo/StartPage";
import { TabBar } from "../features/repo/TabBar";
import { useActiveRepository, useRepoStore } from "../features/repo/store";
import styles from "./App.module.css";
import { Toolbar } from "./Toolbar";

/** Three-zone shell: sidebar · commit graph · contextual detail panel. */
export function App() {
  const { t } = useTranslation();
  const active = useActiveRepository();
  const loadRecents = useRepoStore((s) => s.loadRecents);

  useEffect(() => {
    void loadRecents();
  }, [loadRecents]);

  const sidebarSections = [
    t("sidebar.local"),
    t("sidebar.remote"),
    t("sidebar.tags"),
    t("sidebar.stashes"),
  ];

  return (
    <div className={styles.shell}>
      <Toolbar />
      <TabBar />
      <div className={styles.workspace}>
        <aside className={styles.sidebar} aria-label={t("zones.sidebar")}>
          {sidebarSections.map((title) => (
            <section key={title} className={styles.sidebarSection}>
              <h2 className={styles.sidebarTitle}>{title}</h2>
              <p className={styles.sidebarEmpty}>{t("sidebar.empty")}</p>
            </section>
          ))}
        </aside>
        <main className={styles.graph} aria-label={t("zones.graph")}>
          <OpenErrorBanner />
          {active ? (
            <GraphView key={active.id} repoId={active.id} info={active.info} />
          ) : (
            <StartPage />
          )}
        </main>
        <aside className={styles.detail} aria-label={t("zones.detail")}>
          <p className={styles.detailEmpty}>{t("detail.empty")}</p>
        </aside>
      </div>
    </div>
  );
}
