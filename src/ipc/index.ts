import { open } from "@tauri-apps/plugin-dialog";

export { commands } from "./bindings";
export type {
  ErrorKind,
  IpcError_Serialize as CommandError,
  OpenedRepository,
  RecentRepository,
  RepoInfo,
} from "./bindings";

/** Native folder picker. Resolves to `null` when the user cancels. */
export async function pickFolder(title: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title });
  return typeof selected === "string" ? selected : null;
}
