import { useEffect } from "react";
import { commands } from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useRepoStore } from "../repo/store";
import { useSettingsStore } from "../settings/store";
import { useRemoteStore } from "./store";

/** Fetches every open repository that has a remote, one after another. */
export async function fetchOpenRepositories() {
  for (const tab of useRepoStore.getState().tabs) {
    let refs = useGraphStore.getState().graphs[tab.id]?.refs ?? null;
    if (!refs) {
      // A tab not shown yet this session: ask for its remotes.
      const result = await commands.getRefs(tab.id);
      refs = result.status === "ok" ? result.data : null;
    }
    if (refs && refs.remotes.length > 0) {
      await useRemoteStore.getState().autoFetch(tab.id);
    }
  }
}

/**
 * Settings → General → automatic fetch: every N minutes while the app is
 * open (off by default). Failures are marked next to the branch only.
 */
export function useAutoFetch() {
  const minutes = useSettingsStore((s) => s.general.autoFetchMinutes);
  useEffect(() => {
    if (!minutes) return;
    const timer = window.setInterval(
      () => void fetchOpenRepositories(),
      minutes * 60_000,
    );
    return () => window.clearInterval(timer);
  }, [minutes]);
}
