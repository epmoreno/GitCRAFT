import {
  commands,
  pickFolder,
  streamGraph,
  type CommandError,
  type ErrorKind,
  type GraphEntry,
  type GraphEvent,
  type Lane,
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
};

export function resetIpcMock() {
  for (const fn of Object.values(ipc)) fn.mockReset();
  ipc.listRecentRepositories.mockResolvedValue({ status: "ok", data: [] });
  ipc.closeRepository.mockResolvedValue({ status: "ok", data: true });
  ipc.pickFolder.mockResolvedValue(null);
  ipc.getRefs.mockResolvedValue({ status: "ok", data: emptyRefs() });
  ipc.streamGraph.mockImplementation(async (_id, onEvent) => {
    onEvent({ type: "done", total: 0 });
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
  return { id, info: { path, name, gitDir: `${path}/.git` } };
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

const lane = (column: number, color: number): Lane => ({ column, color });

/** A 40-hex oid made from a short label, e.g. "a" → "aaaa…". */
export const oid = (label: string) => label.repeat(40).slice(0, 40);

/** The documented sample: E, M(B, D), D, C, B, A — main in column 0, feature in 1. */
export function sampleEntries(): GraphEntry[] {
  const time = Math.floor(Date.now() / 1000) - 3 * 24 * 3600;
  const commit = (label: string, parents: string[], summary: string) => ({
    oid: oid(label),
    parents: parents.map(oid),
    authorName: "Ana",
    authorEmail: "ana@example.com",
    authorTime: time,
    commitTime: time,
    summary,
  });
  return [
    {
      commit: commit("e", ["m"], "E"),
      row: {
        column: 0,
        color: 0,
        incoming: [],
        outgoing: [lane(0, 0)],
        passing: [],
      },
    },
    {
      commit: commit("f", ["b", "d"], "Merge branch 'feature'"),
      row: {
        column: 0,
        color: 0,
        incoming: [lane(0, 0)],
        outgoing: [lane(0, 0), lane(1, 1)],
        passing: [],
      },
    },
    {
      commit: commit("d", ["c"], "D"),
      row: {
        column: 1,
        color: 1,
        incoming: [lane(1, 1)],
        outgoing: [lane(1, 1)],
        passing: [lane(0, 0)],
      },
    },
    {
      commit: commit("c", ["a"], "C"),
      row: {
        column: 1,
        color: 1,
        incoming: [lane(1, 1)],
        outgoing: [lane(1, 1)],
        passing: [lane(0, 0)],
      },
    },
    {
      commit: commit("b", ["a"], "B"),
      row: {
        column: 0,
        color: 0,
        incoming: [lane(0, 0)],
        outgoing: [lane(0, 0)],
        passing: [lane(1, 1)],
      },
    },
    {
      commit: commit("a", [], "A"),
      row: {
        column: 0,
        color: 0,
        incoming: [lane(0, 0), lane(1, 1)],
        outgoing: [],
        passing: [],
      },
    },
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
