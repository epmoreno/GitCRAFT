import { create } from "zustand";
import {
  commands,
  streamGraph,
  type CommandError,
  type GraphLine,
  type RepoRefs,
} from "../../ipc";

export type RepoGraph = {
  status: "loading" | "done" | "error";
  lines: GraphLine[];
  /** Widest column seen so far (nodes and lines). */
  maxColumn: number;
  refs: RepoRefs | null;
  error: CommandError | null;
  selectedOid: string | null;
  /** Identifies the load in progress; events from older loads are ignored. */
  loadId: number;
};

type GraphState = {
  graphs: Record<number, RepoGraph>;
  load: (repoId: number) => Promise<void>;
  select: (repoId: number, oid: string | null) => void;
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
          patch(repoId, loadId, () => ({ status: "done" }));
        }
      });
      if (result.status === "error" && result.error.kind !== "cancelled") {
        patch(repoId, loadId, () => ({ status: "error", error: result.error }));
      }
      await refs;
    },

    select: (repoId, oid) => {
      const graph = get().graphs[repoId];
      if (graph) patch(repoId, graph.loadId, () => ({ selectedOid: oid }));
    },

    forget: (repoId) =>
      set((s) => {
        const graphs = { ...s.graphs };
        delete graphs[repoId];
        return { graphs };
      }),
  };
});
