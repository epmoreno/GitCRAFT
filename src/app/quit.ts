/**
 * Quitting the app and closing its window, in one place: ⌘Q / "Quit", the
 * window's close button and the "Quit" of the unsaved-choices warning all
 * end here, and the smoke run (smoke.ts) drives this same path.
 *
 * With a close-requested listener, Tauri always holds the window open and
 * the listener destroys it when the close is not prevented
 * (`Window.onCloseRequested`), which needs the `core:window:allow-destroy`
 * permission (src-tauri/capabilities/default.json).
 */

import {
  reposWithDrafts,
  unsavedPaths,
  useConflictStore,
} from "../features/conflicts/store";
import { useRepoStore } from "../features/repo/store";
import { closeWindow, guardWindowClose } from "./windowClose";

/** Unsaved conflict choices in every tab, as "repo: file". */
export function unsavedEverywhere(): string[] {
  const names = new Map(
    useRepoStore.getState().tabs.map((tab) => [tab.id, tab.info.name]),
  );
  return reposWithDrafts().flatMap((id) =>
    unsavedPaths(id).map((path) => `${names.get(id) ?? id}: ${path}`),
  );
}

let guard: Promise<unknown> | null = null;

/** Asks before the window closes while choices are unsaved. Installed once
 * for the app's lifetime; resolves when it is in place. */
export function installWindowCloseGuard(): Promise<unknown> {
  guard ??= guardWindowClose(() => {
    const paths = unsavedEverywhere();
    if (paths.length === 0) return false;
    useConflictStore.getState().setClosing({ kind: "window", paths });
    return true;
  });
  return guard;
}

/** ⌘Q / "Quit": quits at once, or asks first when choices are unsaved. */
export function requestQuit(): Promise<void> {
  const paths = unsavedEverywhere();
  if (paths.length > 0) {
    useConflictStore.getState().setClosing({ kind: "quit", paths });
    return Promise.resolve();
  }
  return closeWindow();
}

/** The warning's confirm button: closes the tab, or the window (quitting),
 * forgetting the unsaved choices. */
export async function confirmClosing(): Promise<void> {
  const closing = useConflictStore.getState().closing;
  if (!closing) return;
  useConflictStore.getState().setClosing(null);
  if (closing.kind === "tab") {
    useConflictStore.getState().clearRepo(closing.repoId);
    await useRepoStore.getState().closeTab(closing.repoId, true);
  } else {
    useConflictStore.setState({ drafts: {} });
    await closeWindow();
  }
}
