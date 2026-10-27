import { create } from "zustand";
import {
  commands,
  streamGraph,
  type CommandError,
  type GraphEntry,
  type RepoRefs,
} from "../../ipc";

export type RepoGraph = {
  status: "loading" | "done" | "error";
  entries: GraphEntry[];
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
  entries: [],
  maxColumn: 0,
  refs: null,
  error: null,
  selectedOid: null,
  loadId,
});

function widest(entries: GraphEntry[]): number {
  let max = 0;
  for (const { row } of entries) {
    max = Math.max(max, row.column);
    for (const lane of row.passing) max = Math.max(max, lane.column);
    for (const lane of row.outgoing) max = Math.max(max, lane.column);
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
            entries: g.entries.concat(event.entries),
            maxColumn: Math.max(g.maxColumn, widest(event.entries)),
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
