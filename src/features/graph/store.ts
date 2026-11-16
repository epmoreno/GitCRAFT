import { create } from "zustand";
import {
  commands,
  streamGraph,
  type CommandError,
  type GraphLine,
  type RepoRefs,
  type WorktreeArea,
} from "../../ipc";

/** Selection id of the "uncommitted changes" entry (never a commit id). */
export const WORKTREE = "worktree";

const STASH_PREFIX = "stash:";

/** Selection id of a stash entry (shown in the detail panel). */
export const stashSelection = (oid: string) => `${STASH_PREFIX}${oid}`;

/** The stash entry a selection id names, if it names one. */
export const selectedStash = (selection: string | null) =>
  selection?.startsWith(STASH_PREFIX)
    ? selection.slice(STASH_PREFIX.length)
    : null;

/** A file of a commit whose diff is shown over the history. */
export type OpenFile =
  | { kind: "commit"; oid: string; path: string; oldPath: string | null }
  | {
      kind: "worktree";
      area: WorktreeArea;
      path: string;
      oldPath: string | null;
    }
  /** A file in conflict, shown in the conflict view (task 9). */
  | { kind: "conflict"; path: string }
  /** The history of a file (task 12.4), from `rev` (`HEAD` or a commit
   * id) back. `chosen` is the commit picked in it ({@link WORKTREE} for
   * its uncommitted changes), which the detail panel shows. */
  | { kind: "history"; path: string; rev: string; chosen: string | null }
  /** The blame of a file at `rev` (task 12.6). `chosen` is the commit
   * picked in it; `back` the blames to return to ("Blame before this
   * commit" pushes the current one). */
  | {
      kind: "blame";
      path: string;
      rev: string;
      chosen: string | null;
      back: { path: string; rev: string }[];
    };

/** The files whose diff `DiffView` shows. */
export type DiffFile = Exclude<
  OpenFile,
  { kind: "conflict" | "history" | "blame" }
>;

export type RepoGraph = {
  status: "loading" | "done" | "error";
  lines: GraphLine[];
  /** Widest column seen so far (nodes and lines). */
  maxColumn: number;
  refs: RepoRefs | null;
  error: CommandError | null;
  selectedOid: string | null;
  /** The file whose diff is open, if any. */
  openFile: OpenFile | null;
  /** Offer to write a commit-graph (large repo without one). */
  suggestOptimize: boolean;
  /** Identifies the load in progress; events from older loads are ignored. */
  loadId: number;
};

type GraphState = {
  graphs: Record<number, RepoGraph>;
  load: (repoId: number) => Promise<void>;
  /** Reloads after an external change, keeping the current rows (and the
   * selection) on screen until the new history is complete. */
  refresh: (repoId: number) => Promise<void>;
  select: (repoId: number, oid: string | null) => void;
  hideOptimizeNotice: (repoId: number) => void;
  openFileDiff: (repoId: number, file: OpenFile) => void;
  closeFile: (repoId: number) => void;
  /** Picks a commit in the open file history or blame. */
  chooseInHistory: (repoId: number, chosen: string | null) => void;
  /** The open blame moves to `path` at `rev`, remembering where it was. */
  blameBefore: (repoId: number, path: string, rev: string) => void;
  /** The open blame goes back one step, if it can. */
  blameBack: (repoId: number) => void;
  forget: (repoId: number) => void;
};

let nextLoadId = 0;

const emptyGraph = (loadId: number): RepoGraph => ({
  status: "loading",
  lines: [],
  maxColumn: 0,
  refs: null,
  error: null,
  selectedOid: null,
  suggestOptimize: false,
  openFile: null,
  loadId,
});

function widest(lines: GraphLine[]): number {
  let max = 0;
  for (const line of lines) {
    max = Math.max(max, line.column);
    for (const [column] of line.passing) max = Math.max(max, column);
    for (const [column] of line.outgoing) max = Math.max(max, column);
  }
  return max;
}

export const useGraphStore = create<GraphState>()((set, get) => {
  /** Applies `update` only if `loadId` is still the current load. */
  const patch = (
    repoId: number,
    loadId: number,
    update: (g: RepoGraph) => Partial<RepoGraph>,
  ) =>
    set((s) => {
      const graph = s.graphs[repoId];
      if (!graph || graph.loadId !== loadId) return s;
      return {
        graphs: { ...s.graphs, [repoId]: { ...graph, ...update(graph) } },
      };
    });

  return {
    graphs: {},

    load: async (repoId) => {
      const loadId = ++nextLoadId;
      set((s) => ({ graphs: { ...s.graphs, [repoId]: emptyGraph(loadId) } }));

      const refs = commands.getRefs(repoId).then((result) => {
        if (result.status === "ok") {
          patch(repoId, loadId, () => ({ refs: result.data }));
        }
      });

      const result = await streamGraph(repoId, (event) => {
        if (event.type === "page") {
          patch(repoId, loadId, (g) => ({
            lines: g.lines.concat(event.lines),
            maxColumn: Math.max(g.maxColumn, widest(event.lines)),
          }));
        } else {
          patch(repoId, loadId, () => ({
            status: "done",
            suggestOptimize: event.suggestOptimize,
          }));
        }
      });
      if (result.status === "error" && result.error.kind !== "cancelled") {
        patch(repoId, loadId, () => ({ status: "error", error: result.error }));
      }
      await refs;
    },

    refresh: async (repoId) => {
      const current = get().graphs[repoId];
      if (!current) return;
      // The history is fully determined by the refs: when none moved (e.g.
      // the second notice after our own commit), there is nothing to reload.
      if (current.status === "done" && current.refs) {
        const latest = await commands.getRefs(repoId);
        if (
          latest.status === "ok" &&
          JSON.stringify(latest.data) === JSON.stringify(current.refs)
        ) {
          return;
        }
      }
      const loadId = ++nextLoadId;
      set((s) => {
        const graph = s.graphs[repoId];
        return graph
          ? { graphs: { ...s.graphs, [repoId]: { ...graph, loadId } } }
          : s;
      });

      const refs = commands.getRefs(repoId).then((result) => {
        if (result.status === "ok") {
          patch(repoId, loadId, () => ({ refs: result.data }));
        }
      });
      const fresh: GraphLine[] = [];
      const result = await streamGraph(repoId, (event) => {
        if (event.type === "page") {
          fresh.push(...event.lines);
        } else {
          patch(repoId, loadId, () => ({
            status: "done",
            lines: fresh,
            maxColumn: widest(fresh),
            error: null,
            suggestOptimize: event.suggestOptimize,
          }));
        }
      });
      if (result.status === "error" && result.error.kind !== "cancelled") {
        patch(repoId, loadId, () => ({ status: "error", error: result.error }));
      }
      await refs;
    },

    select: (repoId, oid) => {
      const graph = get().graphs[repoId];
      if (graph) {
        // Picking another commit closes the diff of the previous one.
        patch(repoId, graph.loadId, (g) => ({
          selectedOid: oid,
          openFile: g.selectedOid === oid ? g.openFile : null,
        }));
      }
    },

    openFileDiff: (repoId, file) => {
      const graph = get().graphs[repoId];
      if (graph) patch(repoId, graph.loadId, () => ({ openFile: file }));
    },

    chooseInHistory: (repoId, chosen) => {
      const graph = get().graphs[repoId];
      const open = graph?.openFile;
      if (!graph || (open?.kind !== "history" && open?.kind !== "blame")) {
        return;
      }
      patch(repoId, graph.loadId, () => ({ openFile: { ...open, chosen } }));
    },

    blameBefore: (repoId, path, rev) => {
      const graph = get().graphs[repoId];
      const open = graph?.openFile;
      if (!graph || open?.kind !== "blame") return;
      patch(repoId, graph.loadId, () => ({
        openFile: {
          kind: "blame",
          path,
          rev,
          chosen: null,
          back: [...open.back, { path: open.path, rev: open.rev }],
        },
      }));
    },

    blameBack: (repoId) => {
      const graph = get().graphs[repoId];
      const open = graph?.openFile;
      if (!graph || open?.kind !== "blame") return;
      const previous = open.back.at(-1);
      if (!previous) return;
      patch(repoId, graph.loadId, () => ({
        openFile: {
          kind: "blame",
          ...previous,
          chosen: null,
          back: open.back.slice(0, -1),
        },
      }));
    },

    closeFile: (repoId) => {
      const graph = get().graphs[repoId];
      if (graph) patch(repoId, graph.loadId, () => ({ openFile: null }));
    },

    hideOptimizeNotice: (repoId) => {
      const graph = get().graphs[repoId];
      if (graph) {
        patch(repoId, graph.loadId, () => ({ suggestOptimize: false }));
      }
    },

    forget: (repoId) =>
      set((s) => {
        const graphs = { ...s.graphs };
        delete graphs[repoId];
        return { graphs };
      }),
  };
});
