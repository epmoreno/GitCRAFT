import {
  commands,
  onRepoChanged,
  onCommandRecord,
  fetchRemote,
  optimizeRepository,
  pullBranch,
  pushBranch,
  forcePushPreview,
  cloneRepository,
  pickFolder,
  setUiZoom,
  streamGraph,
  type CommandError,
  type ErrorKind,
  type FileDiff,
  type GeneralSettings,
  type GraphEvent,
  type GraphLine,
  type OpenedRepository,
  type RepoRefs,
  type WorkingStatus,
} from "../ipc";

/** Typed handles to the mocked IPC module (see setup.ts). */
export const ipc = {
  openRepository: vi.mocked(commands.openRepository),
  closeRepository: vi.mocked(commands.closeRepository),
  listRecentRepositories: vi.mocked(commands.listRecentRepositories),
  getRefs: vi.mocked(commands.getRefs),
  pickFolder: vi.mocked(pickFolder),
  streamGraph: vi.mocked(streamGraph),
  optimizeRepository: vi.mocked(optimizeRepository),
  fetchRemote: vi.mocked(fetchRemote),
  pullBranch: vi.mocked(pullBranch),
  pushBranch: vi.mocked(pushBranch),
  forcePushPreview: vi.mocked(forcePushPreview),
  getPullMode: vi.mocked(commands.getPullMode),
  setPullMode: vi.mocked(commands.setPullMode),
  abortInProgress: vi.mocked(commands.abortInProgress),
  saveSession: vi.mocked(commands.saveSession),
  restoreSession: vi.mocked(commands.restoreSession),
  checkInit: vi.mocked(commands.checkInit),
  validBranchName: vi.mocked(commands.validBranchName),
  initRepository: vi.mocked(commands.initRepository),
  cloneRepository: vi.mocked(cloneRepository),
  cloneParent: vi.mocked(commands.cloneParent),
  defaultCloneName: vi.mocked(commands.defaultCloneName),
  checkCloneDestination: vi.mocked(commands.checkCloneDestination),
  cancelClone: vi.mocked(commands.cancelClone),
  onRepoChanged: vi.mocked(onRepoChanged),
  onCommandRecord: vi.mocked(onCommandRecord),
  commandHistory: vi.mocked(commands.commandHistory),
  clearCommandHistory: vi.mocked(commands.clearCommandHistory),
  cancelOperation: vi.mocked(commands.cancelOperation),
  dismissOptimizeNotice: vi.mocked(commands.dismissOptimizeNotice),
  commitDetails: vi.mocked(commands.commitDetails),
  commitChanges: vi.mocked(commands.commitChanges),
  commitFileDiff: vi.mocked(commands.commitFileDiff),
  workingStatus: vi.mocked(commands.workingStatus),
  stageFiles: vi.mocked(commands.stageFiles),
  unstageFiles: vi.mocked(commands.unstageFiles),
  createCommit: vi.mocked(commands.createCommit),
  amendInfo: vi.mocked(commands.amendInfo),
  worktreeFileDiff: vi.mocked(commands.worktreeFileDiff),
  applySelection: vi.mocked(commands.applySelection),
  discardPreview: vi.mocked(commands.discardPreview),
  discardChanges: vi.mocked(commands.discardChanges),
  undoDiscard: vi.mocked(commands.undoDiscard),
  checkBranchName: vi.mocked(commands.checkBranchName),
  createBranch: vi.mocked(commands.createBranch),
  renameBranch: vi.mocked(commands.renameBranch),
  branchDeletionPreview: vi.mocked(commands.branchDeletionPreview),
  deleteBranch: vi.mocked(commands.deleteBranch),
  checkoutBranch: vi.mocked(commands.checkoutBranch),
  stashAndCheckout: vi.mocked(commands.stashAndCheckout),
  listStashes: vi.mocked(commands.listStashes),
  createStash: vi.mocked(commands.createStash),
  applyStash: vi.mocked(commands.applyStash),
  dropStash: vi.mocked(commands.dropStash),
  recoverPreview: vi.mocked(commands.recoverPreview),
  recoverFile: vi.mocked(commands.recoverFile),
  generalSettings: vi.mocked(commands.generalSettings),
  setGeneralSettings: vi.mocked(commands.setGeneralSettings),
  setUiZoom: vi.mocked(setUiZoom),
};

export function resetIpcMock() {
  for (const fn of Object.values(ipc)) fn.mockReset();
  ipc.listRecentRepositories.mockResolvedValue({ status: "ok", data: [] });
  ipc.closeRepository.mockResolvedValue({ status: "ok", data: true });
  ipc.pickFolder.mockResolvedValue(null);
  ipc.onRepoChanged.mockResolvedValue(() => {});
  ipc.onCommandRecord.mockResolvedValue(() => {});
  ipc.commandHistory.mockResolvedValue({ status: "ok", data: [] });
  ipc.clearCommandHistory.mockResolvedValue({ status: "ok", data: null });
  ipc.workingStatus.mockResolvedValue({ status: "ok", data: cleanStatus() });
  ipc.stageFiles.mockResolvedValue({ status: "ok", data: null });
  ipc.unstageFiles.mockResolvedValue({ status: "ok", data: null });
  ipc.createCommit.mockResolvedValue({ status: "ok", data: oid("9") });
  ipc.amendInfo.mockResolvedValue({ status: "ok", data: null });
  ipc.applySelection.mockResolvedValue({ status: "ok", data: null });
  ipc.undoDiscard.mockResolvedValue({ status: "ok", data: null });
  ipc.checkBranchName.mockResolvedValue({ status: "ok", data: "valid" });
  ipc.listStashes.mockResolvedValue({ status: "ok", data: [] });
  ipc.createStash.mockResolvedValue({ status: "ok", data: oid("5") });
  ipc.applyStash.mockResolvedValue({
    status: "ok",
    data: { type: "applied", dropped: false },
  });
  ipc.dropStash.mockResolvedValue({ status: "ok", data: null });
  ipc.recoverFile.mockResolvedValue({ status: "ok", data: null });
  ipc.createBranch.mockResolvedValue({ status: "ok", data: null });
  ipc.renameBranch.mockResolvedValue({ status: "ok", data: null });
  ipc.deleteBranch.mockResolvedValue({ status: "ok", data: null });
  ipc.checkoutBranch.mockResolvedValue({
    status: "ok",
    data: { type: "switched" },
  });
  ipc.stashAndCheckout.mockResolvedValue({
    status: "ok",
    data: { type: "switched" },
  });
  ipc.commitChanges.mockResolvedValue({
    status: "ok",
    data: { base: null, files: [] },
  });
  ipc.cancelOperation.mockResolvedValue({ status: "ok", data: null });
  ipc.generalSettings.mockResolvedValue({
    status: "ok",
    data: defaultGeneralSettings(),
  });
  ipc.setGeneralSettings.mockResolvedValue({ status: "ok", data: null });
  ipc.setUiZoom.mockResolvedValue(undefined);
  ipc.fetchRemote.mockResolvedValue({ status: "ok", data: null });
  ipc.getPullMode.mockResolvedValue({
    status: "ok",
    data: { mode: "merge", configured: "merge", chosen: false },
  });
  ipc.setPullMode.mockResolvedValue({ status: "ok", data: null });
  ipc.abortInProgress.mockResolvedValue({ status: "ok", data: null });
  ipc.saveSession.mockResolvedValue({ status: "ok", data: null });
  ipc.validBranchName.mockResolvedValue({ status: "ok", data: true });
  ipc.cloneParent.mockResolvedValue({ status: "ok", data: "/Users/ana" });
  ipc.defaultCloneName.mockImplementation(async (url) => ({
    status: "ok",
    data:
      url
        .split("/")
        .pop()
        ?.replace(/\.git$/, "") ?? "",
  }));
  ipc.checkCloneDestination.mockResolvedValue({ status: "ok", data: "free" });
  ipc.cancelClone.mockResolvedValue({ status: "ok", data: null });
  ipc.restoreSession.mockResolvedValue({
    status: "ok",
    data: { opened: [], active: null, failed: [] },
  });
  ipc.pushBranch.mockResolvedValue({
    status: "ok",
    data: {
      type: "pushed",
      upstream: "origin/main",
      commits: 1,
      setUpstream: false,
      forced: false,
    },
  });
  ipc.pullBranch.mockResolvedValue({
    status: "ok",
    data: {
      type: "pulled",
      upstream: "origin/main",
      commits: 1,
      mode: "merge",
      fastForward: true,
    },
  });
  ipc.dismissOptimizeNotice.mockResolvedValue({ status: "ok", data: null });
  ipc.getRefs.mockResolvedValue({ status: "ok", data: emptyRefs() });
  ipc.streamGraph.mockImplementation(async (_id, onEvent) => {
    onEvent({ type: "done", total: 0, suggestOptimize: false });
    return { status: "ok", data: null };
  });
}

/** Makes `streamGraph` deliver these events, then resolve. */
export function streamEvents(events: GraphEvent[]) {
  ipc.streamGraph.mockImplementation(async (_id, onEvent) => {
    for (const event of events) onEvent(event);
    return { status: "ok", data: null };
  });
}

export function opened(id: number, path: string): OpenedRepository {
  const name = path.split("/").pop() ?? path;
  return {
    id,
    info: { path, name, gitDir: `${path}/.git`, commonDir: `${path}/.git` },
  };
}

export function commandError(
  kind: ErrorKind,
  details: string | null = null,
): CommandError {
  return { kind, message: `${kind} (test)`, details };
}

export function defaultGeneralSettings(): GeneralSettings {
  return {
    language: null,
    textSize: "normal",
    cloneFolder: null,
    autoFetchMinutes: null,
  };
}

export function cleanStatus(): WorkingStatus {
  return {
    branch: {
      name: "main",
      oid: oid("e"),
      upstream: null,
      ahead: 0,
      behind: 0,
    },
    staged: [],
    unstaged: [],
    untracked: [],
    conflicted: [],
    inProgress: null,
  };
}

export function emptyRefs(): RepoRefs {
  return {
    head: { type: "branch", name: "main", oid: null },
    refs: [],
    remotes: [],
  };
}

/** A 40-hex oid made from a short label, e.g. "a" → "aaaa…". */
export const oid = (label: string) => label.repeat(40).slice(0, 40);

/** The documented sample: E, M(B, D), D, C, B, A — main in column 0, feature in 1. */
export function sampleLines(): GraphLine[] {
  const time = Math.floor(Date.now() / 1000) - 3 * 24 * 3600;
  const line = (
    label: string,
    summary: string,
    column: number,
    lanes: Pick<GraphLine, "incoming" | "outgoing" | "passing">,
    merge = false,
  ): GraphLine => ({
    oid: oid(label),
    summary,
    author: "Ana",
    time,
    merge,
    column,
    color: column,
    ...lanes,
  });
  return [
    line("e", "E", 0, { incoming: [], outgoing: [[0, 0]], passing: [] }),
    line(
      "f",
      "Merge branch 'feature'",
      0,
      {
        incoming: [[0, 0]],
        outgoing: [
          [0, 0],
          [1, 1],
        ],
        passing: [],
      },
      true,
    ),
    line("d", "D", 1, {
      incoming: [[1, 1]],
      outgoing: [[1, 1]],
      passing: [[0, 0]],
    }),
    line("c", "C", 1, {
      incoming: [[1, 1]],
      outgoing: [[1, 1]],
      passing: [[0, 0]],
    }),
    line("b", "B", 0, {
      incoming: [[0, 0]],
      outgoing: [[0, 0]],
      passing: [[1, 1]],
    }),
    line("a", "A", 0, {
      incoming: [
        [0, 0],
        [1, 1],
      ],
      outgoing: [],
      passing: [],
    }),
  ];
}

export function sampleRefs(): RepoRefs {
  const ref = (
    shortName: string,
    kind: "local" | "remote" | "tag",
    target: string,
    extra: Partial<RepoRefs["refs"][number]> = {},
  ) => ({
    name: `refs/${kind === "local" ? "heads" : kind === "remote" ? "remotes" : "tags"}/${shortName}`,
    shortName,
    kind,
    target: oid(target),
    remote: kind === "remote" ? "origin" : null,
    upstream: null,
    isHead: false,
    ahead: null,
    behind: null,
    upstreamGone: false,
    ...extra,
  });
  return {
    head: { type: "branch", name: "main", oid: oid("e") },
    refs: [
      ref("main", "local", "e", {
        isHead: true,
        upstream: "origin/main",
        ahead: 0,
        behind: 0,
      }),
      ref("feature", "local", "d"),
      ref("origin/main", "remote", "e"),
      ref("v1.0", "tag", "b"),
    ],
    remotes: ["origin"],
  };
}

/** A one-hunk diff: two → TWO, between context lines. */
export const textDiff = (overrides: Partial<FileDiff> = {}): FileDiff => ({
  binary: false,
  tooLarge: false,
  lineCount: 4,
  notes: [],
  hunks: [
    {
      oldStart: 1,
      oldLines: 3,
      newStart: 1,
      newLines: 3,
      section: "fn main() {",
      lines: [
        {
          kind: "context",
          content: "one",
          oldLine: 1,
          newLine: 1,
          noNewline: false,
        },
        {
          kind: "removed",
          content: "two",
          oldLine: 2,
          newLine: null,
          noNewline: false,
        },
        {
          kind: "added",
          content: "TWO",
          oldLine: null,
          newLine: 2,
          noNewline: false,
        },
        {
          kind: "context",
          content: "three",
          oldLine: 3,
          newLine: 3,
          noNewline: true,
        },
      ],
    },
  ],
  fingerprint: "f1",
  ...overrides,
});
