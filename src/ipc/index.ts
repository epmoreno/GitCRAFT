import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, type GraphEvent } from "./bindings";

export { commands } from "./bindings";
export type {
  Commit,
  ErrorKind,
  GraphEntry,
  GraphEvent,
  GraphRow,
  IpcError_Serialize as CommandError,
  GitRef_Serialize as GitRef,
  Head_Serialize as Head,
  Lane,
  OpenedRepository,
  RecentRepository,
  RefKind,
  RepoInfo,
  RepoRefs_Serialize as RepoRefs,
} from "./bindings";

/** Native folder picker. Resolves to `null` when the user cancels. */
export async function pickFolder(title: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title });
  return typeof selected === "string" ? selected : null;
}

/** Streams the commit graph of a session: `page` events, then `done`. */
export function streamGraph(id: number, onEvent: (event: GraphEvent) => void) {
  const channel = new Channel<GraphEvent>();
  channel.onmessage = onEvent;
  return commands.loadGraph(id, channel);
}
