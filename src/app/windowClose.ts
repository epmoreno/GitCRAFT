import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Asks `shouldKeepOpen` before the window closes (its close button, or
 * "Quit" on Windows and Linux): if it answers true the close is cancelled
 * and the caller shows its own question. Separate module so tests can
 * replace it.
 */
export async function guardWindowClose(
  shouldKeepOpen: () => boolean,
): Promise<() => void> {
  return getCurrentWindow().onCloseRequested((event) => {
    if (shouldKeepOpen()) event.preventDefault();
  });
}

/** Closes the window (after the user confirmed). */
export async function closeWindow(): Promise<void> {
  await getCurrentWindow().close();
}
