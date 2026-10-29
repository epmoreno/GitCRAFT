import { Channel } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  commands,
  events,
  type GraphEvent,
  type RepoChanged,
} from "./bindings";

export { commands } from "./bindings";
export type {
  AmendInfo,
  ApplyOutcome,
  BlockingFile,
  BranchDeletion_Serialize as BranchDeletion,
  BranchNameCheck,
  CheckoutOutcome,
  OrphanCommit,
  CommitChanges_Serialize as CommitChanges,
  ConflictEntry,
  ConflictKind,
  DiscardResult_Serialize as DiscardResult,
  DiscardTarget_Deserialize as DiscardTarget,
  DiscardedFile_Serialize as DiscardedFile,
  CommitDetails,
  ErrorKind,
  FileChange_Serialize as FileChange,
  FileDiff_Serialize as FileDiff,
  FileStatus,
  GraphEvent,
  GraphLine,
  IpcError_Serialize as CommandError,
  GitRef_Serialize as GitRef,
  Head_Serialize as Head,
  OpenedRepository,
  RecentRepository,
  RestoredSession_Serialize as RestoredSession,
  FailedRestore_Serialize as FailedRestore,
  RecoverPreview_Serialize as RecoverPreview,
  RefKind,
  RepoInfo,
  RepoChanged,
  RepoRefs_Serialize as RepoRefs,
  Signature,
  StashEntry_Serialize as StashEntry,
  StashOrigin,
  StatusEntry_Serialize as StatusEntry,
  WorkingStatus_Serialize as WorkingStatus,
  WorktreeArea,
  HunkSelection_Deserialize as HunkSelection,
  InitCheck_Serialize as InitCheck,
  Destination,
  InProgress,
  PullBlock,
  PullMode,
  PullModeInfo,
  PullOutcome,
  PushOutcome,
  PushReject,
  ForcePushPreview,
  SelectionAction,
} from "./bindings";

/** A graph line: `[column, colour]`. */
export type Lane = [column: number, color: number];

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

/** Writes the repository's commit-graph, reporting git's progress lines. */
export function optimizeRepository(
  id: number,
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.optimizeRepository(id, channel);
}

/** Fetches from `remote` (every remote when `null`), reporting git's
 * progress lines. Cancel with `commands.cancelOperation`. */
export function fetchRemote(
  id: number,
  remote: string | null,
  prune: boolean,
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.fetchRemote(id, remote, prune, channel);
}

/** Pulls the current branch, reporting git's progress lines. */
export function pullBranch(
  id: number,
  options: { allowConflicts: boolean; saveLocal: boolean },
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.pullBranch(
    id,
    options.allowConflicts,
    options.saveLocal,
    channel,
  );
}

/** Pushes the current branch (or publishes it to `publishTo`); forced
 * only with a lease (`forceLease`: the remote commit it must still be at). */
export function pushBranch(
  id: number,
  options: { publishTo: string | null; forceLease: string | null },
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.pushBranch(
    id,
    options.publishTo,
    options.forceLease,
    channel,
  );
}

/** Fetches, then lists what a forced push would overwrite. */
export function forcePushPreview(
  id: number,
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.forcePushPreview(id, channel);
}

/** Clones `url` into `parent/name` (and opens it), reporting git's progress.
 * Cancel with `commands.cancelClone`. */
export function cloneRepository(
  url: string,
  parent: string,
  name: string,
  onProgress: (line: string) => void,
) {
  const channel = new Channel<string>();
  channel.onmessage = onProgress;
  return commands.cloneRepository(url, parent, name, channel);
}

/** Calls `handler` whenever an open repository changes outside GitCRAFT
 * (refs and/or working directory). Resolves to an unsubscribe function. */
export function onRepoChanged(handler: (change: RepoChanged) => void) {
  return events.repoChanged.listen((event) => handler(event.payload));
}
