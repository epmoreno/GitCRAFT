import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { BranchDialogs, BranchToast } from "../features/branches/BranchDialogs";
import { CreateDialogs } from "../features/create/CreateDialogs";
import { DetailPanel } from "../features/detail/DetailPanel";
import { RemoteDialogs } from "../features/remote/RemoteDialogs";
import { useAutoFetch } from "../features/remote/autoFetch";
import { RemoteProgress, RemoteToast } from "../features/remote/RemoteStatus";
import { StashDialogs, StashToast } from "../features/stash/StashDialogs";
import { useStashStore } from "../features/stash/store";
import { GraphView } from "../features/graph/GraphView";
import { useGraphStore } from "../features/graph/store";
import { DiscardDialog, UndoToast } from "../features/workdir/Discard";
import { useWorkdirStore } from "../features/workdir/store";
import { onCommandRecord, onRepoChanged } from "../ipc";
import { OperationsPanel } from "../features/operations/OperationsPanel";
import { useOperationsStore } from "../features/operations/store";
import {
  OpenErrorBanner,
  RestoreFailedBanner,
} from "../features/repo/OpenErrorBanner";
import { installAppMenu } from "./menu";
import { installWindowCloseGuard, requestQuit } from "./quit";
import { InProgressBanner } from "../features/conflicts/OperationBanner";
import { UnsavedDialog } from "../features/conflicts/UnsavedDialog";
import {
  HistoryDialogs,
  HistoryToast,
} from "../features/history/HistoryDialogs";
import { TagDialogs, TagToast } from "../features/history/TagDialogs";
import { MergeDialog } from "../features/merge/MergeDialog";
import { reportIfSmokeRun } from "./smoke";
import { isShortcut } from "./shortcuts";
import { StartPage } from "../features/repo/StartPage";
import { PublishDialog } from "../features/publish/PublishDialog";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { useSettingsStore } from "../features/settings/store";
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
  const restoreSession = useRepoStore((s) => s.restoreSession);
  const restored = useRepoStore((s) => s.restored);
  useAutoFetch();

  useEffect(() => {
    void useSettingsStore.getState().load();
    void restoreSession();
    void loadRecents();
    void reportIfSmokeRun();
  }, [restoreSession, loadRecents]);

  // Closing the window with unsaved conflict choices asks first.
  useEffect(() => {
    installWindowCloseGuard().catch((error: unknown) => {
      console.warn("could not guard the window close", error);
    });
  }, []);
  // ⌘T / Ctrl+T opens a new tab and ⌘W / Ctrl+W closes the active one
  // (menu bar); ⌘1…⌘9 and ⌃Tab / ⌃⇧Tab switch tabs. Never while a dialog
  // waits for an answer.
  useEffect(() => {
    const dialogOpen = () => document.querySelector('[aria-modal="true"]');
    const tabs = useRepoStore.getState;
    void installAppMenu(t, {
      openSettings: () => {
        if (!dialogOpen()) useSettingsStore.getState().openSettings();
      },
      newTab: () => {
        if (!dialogOpen()) tabs().newTab();
      },
      closeTab: () => {
        if (!dialogOpen()) void tabs().closeActiveTab();
      },
      quit: () => void requestQuit(),
    }).catch((error: unknown) => {
      console.warn("could not install the app menu", error);
    });
    const onKey = (event: KeyboardEvent) => {
      if (dialogOpen()) return;
      if (isShortcut("tabByNumber", event)) {
        event.preventDefault();
        tabs().activateIndex(Number(event.key) - 1);
      } else if (isShortcut("nextTab", event)) {
        event.preventDefault();
        tabs().activateNext(1);
      } else if (isShortcut("previousTab", event)) {
        event.preventDefault();
        tabs().activateNext(-1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [t]);

  // Changes made outside GitCRAFT (terminal, editor, another client).
  useEffect(() => {
    const operations = useOperationsStore.getState();
    const unlisten = onCommandRecord(operations.receive);
    void operations.load();
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => {
    const unlisten = onRepoChanged(({ id, refs, worktree, stash }) => {
      if (refs) void useGraphStore.getState().refresh(id);
      if (worktree) void useWorkdirStore.getState().refresh(id);
      if (stash) void useStashStore.getState().refresh(id);
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
    void useStashStore.getState().refresh(activeId);
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
          <RestoreFailedBanner />
          <InProgressBanner repoId={active?.id ?? null} />
          {active ? (
            <GraphView key={active.id} repoId={active.id} info={active.info} />
          ) : restored ? (
            <StartPage />
          ) : null}
          <UndoToast />
          <BranchToast />
          <StashToast />
          <RemoteToast />
          <RemoteProgress />
          <HistoryToast repoId={active?.id ?? null} />
          <TagToast repoId={active?.id ?? null} />
        </main>
        <DetailPanel repoId={active?.id ?? null} />
        <DiscardDialog />
        <BranchDialogs />
        <StashDialogs />
        <RemoteDialogs />
        <CreateDialogs />
        <SettingsDialog />
        <PublishDialog />
        <UnsavedDialog />
        <HistoryDialogs />
        <TagDialogs />
        <MergeDialog />
      </div>
      <OperationsPanel />
    </div>
  );
}
