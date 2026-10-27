import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { GraphView } from "../features/graph/GraphView";
import { useGraphStore } from "../features/graph/store";
import { onRepoChanged } from "../ipc";
import { OpenErrorBanner } from "../features/repo/OpenErrorBanner";
import { StartPage } from "../features/repo/StartPage";
import { Sidebar } from "../features/sidebar/Sidebar";
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

  // Refs changed outside GitCRAFT (terminal, another client): refresh.
  useEffect(() => {
    const unlisten = onRepoChanged((id) => {
      void useGraphStore.getState().refresh(id);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div className={styles.shell}>
      <Toolbar />
      <TabBar />
      <div className={styles.workspace}>
        <Sidebar repoId={active?.id ?? null} />
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
