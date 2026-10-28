import {
  commands,
  onRepoChanged,
  optimizeRepository,
  pickFolder,
  streamGraph,
  type CommandError,
  type ErrorKind,
  type GraphEvent,
  type GraphLine,
  type OpenedRepository,
  type RepoRefs,
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
  onRepoChanged: vi.mocked(onRepoChanged),
  cancelOperation: vi.mocked(commands.cancelOperation),
  dismissOptimizeNotice: vi.mocked(commands.dismissOptimizeNotice),
  commitDetails: vi.mocked(commands.commitDetails),
  commitChanges: vi.mocked(commands.commitChanges),
};

export function resetIpcMock() {
  for (const fn of Object.values(ipc)) fn.mockReset();
  ipc.listRecentRepositories.mockResolvedValue({ status: "ok", data: [] });
  ipc.closeRepository.mockResolvedValue({ status: "ok", data: true });
  ipc.pickFolder.mockResolvedValue(null);
  ipc.onRepoChanged.mockResolvedValue(() => {});
  ipc.commitChanges.mockResolvedValue({
    status: "ok",
    data: { base: null, files: [] },
  });
  ipc.cancelOperation.mockResolvedValue({ status: "ok", data: null });
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
    ...extra,
  });
  return {
    head: { type: "branch", name: "main", oid: oid("e") },
    refs: [
      ref("main", "local", "e", { isHead: true, upstream: "origin/main" }),
      ref("feature", "local", "d"),
      ref("origin/main", "remote", "e"),
      ref("v1.0", "tag", "b"),
    ],
    remotes: ["origin"],
  };
}
