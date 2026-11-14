import { Channel } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  commands,
  events,
  type CommandRecord_Serialize as CommandRecord,
  type GraphEvent,
  type LoginEvent,
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
  CommandOutcome_Serialize as CommandOutcome,
  CommandRecord_Serialize as CommandRecord,
  ConflictEntry,
  ConflictBlock_Serialize as ConflictBlock,
  ConflictContent_Serialize as ConflictContent,
  ConflictFile_Serialize as ConflictFile,
  ConflictSides_Serialize as ConflictSides,
  ConflictVersion_Serialize as ConflictVersion,
  CommitRef,
  CommitActions,
  PickBlock,
  PickKind,
  PickOutcome_Serialize as PickOutcome,
  PickPreview_Serialize as PickPreview,
  PickRefusal,
  LeavingCommit,
  ResetMode,
  ResetPreview_Serialize as ResetPreview,
  ResetRefusal,
  ResetResult_Serialize as ResetResult,
  RemoteTag,
  TagInfo,
  TagNameCheck,
  TagPushOutcome,
  ContinueOutcome,
  EntryKind,
  MarkOutcome,
  MarkerLine,
  NamedCommit,
  RebaseProgress,
  ResolvedFile,
  Segment_Serialize as Segment,
  Side,
  WholeFileReason,
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
  GeneralSettings_Serialize as GeneralSettings,
  GlobalDefaults_Serialize as GlobalDefaults,
  PullStrategy,
  ConfigChange_Serialize as ConfigChange,
  EffectiveValue,
  Identity_Serialize as Identity,
  IdentityInfo_Serialize as IdentityInfo,
  GhStatus_Serialize as GhStatus,
  GhAccount,
  LoginEvent,
  PublishInfo_Serialize as PublishInfo,
  Published,
  PublishPreview,
  Language as SettingsLanguage,
  TextSize,
} from "./bindings";

/** A graph line: `[column, colour]`. */
export type Lane = [column: number, color: number];

/** Scales the whole UI (Settings → General → Text size). */
export async function setUiZoom(factor: number): Promise<void> {
  await getCurrentWebview().setZoom(factor);
}

/** GitHub sign-in through gh; `onEvent` gets the one-time code. */
export function ghLogin(onEvent: (event: LoginEvent) => void) {
  const channel = new Channel<LoginEvent>();
  channel.onmessage = onEvent;
  return commands.ghLogin(channel);
}

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

/** A git command started or finished (for the operations panel). */
export function onCommandRecord(handler: (record: CommandRecord) => void) {
  return events.commandRecord.listen((event) => handler(event.payload));
}
