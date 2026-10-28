import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { BranchDialogs, BranchToast } from "../features/branches/BranchDialogs";
import { DetailPanel } from "../features/detail/DetailPanel";
import { GraphView } from "../features/graph/GraphView";
import { useGraphStore } from "../features/graph/store";
import { DiscardDialog, UndoToast } from "../features/workdir/Discard";
import { useWorkdirStore } from "../features/workdir/store";
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

  // Changes made outside GitCRAFT (terminal, editor, another client).
  useEffect(() => {
    const unlisten = onRepoChanged(({ id, refs, worktree }) => {
      if (refs) void useGraphStore.getState().refresh(id);
      if (worktree) void useWorkdirStore.getState().refresh(id);
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  // The working directory status of the active tab: on activation and
  // whenever the window regains focus (cheap, catches anything missed).
  const activeId = active?.id ?? null;
  useEffect(() => {
    if (activeId === null) return;
    const refresh = () => void useWorkdirStore.getState().refresh(activeId);
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [activeId]);

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
          <UndoToast />
          <BranchToast />
        </main>
        <DetailPanel repoId={active?.id ?? null} />
        <DiscardDialog />
        <BranchDialogs />
      </div>
    </div>
  );
}
