import { create } from "zustand";
import {
  commands,
  searchCommits,
  type CommandError,
  type SearchField,
  type SearchQuery,
} from "../../ipc";

/**
 * The history search of each repository tab (task 12.2): what is being
 * looked for, the results as they arrive and which one is shown. Each tab
 * keeps its own; the search itself runs in the backend (task 12.1).
 */

export type SearchStatus =
  /** Nothing to search (closed, empty text or not a hash). */
  "idle" | "running" | "done" | "error";

export type RepoSearch = {
  open: boolean;
  text: string;
  field: SearchField;
  regex: boolean;
  matchCase: boolean;
  status: SearchStatus;
  /** The commits found. Mutated in place as batches arrive; `version`
   * changes with it, so subscribe to that. */
  matches: Set<string>;
  version: number;
  found: number;
  /** Commits examined, when the search can tell (`progressKnown`). */
  examined: number;
  progressKnown: boolean;
  /** A jump waiting for the graph to load the next result. */
  pending: "next" | "previous" | null;
  /** An id no branch has (only in the branch history). */
  outside: string | null;
  error: CommandError | null;
  /** Bumped to put the focus in the field. */
  focusRequest: number;
};

type SearchState = {
  searches: Record<number, RepoSearch>;
  /** Opens the bar (or focuses it, if open). */
  openBar: (repoId: number) => void;
  close: (repoId: number) => void;
  setQuery: (
    repoId: number,
    query: Partial<Pick<RepoSearch, "text" | "field" | "regex" | "matchCase">>,
  ) => void;
  /** Runs the search for the current query (replacing any running). */
  run: (repoId: number) => Promise<void>;
  /** Stops the running search (switching tabs); it runs again later. */
  stop: (repoId: number) => void;
  setPending: (repoId: number, pending: RepoSearch["pending"]) => void;
  forget: (repoId: number) => void;
};

export const initialSearchState = { searches: {} };

const fresh = (): RepoSearch => ({
  open: false,
  text: "",
  field: "message",
  regex: false,
  matchCase: false,
  status: "idle",
  matches: new Set(),
  version: 0,
  found: 0,
  examined: 0,
  progressKnown: false,
  pending: null,
  outside: null,
  error: null,
  focusRequest: 0,
});

/** Shortest hash prefix searched (as in the backend). */
export const MIN_HASH_LENGTH = 4;

/** Why the text cannot be searched as a hash, if it cannot. */
export function hashProblem(text: string): "short" | "notHex" | null {
  const value = text.trim();
  if (!/^[0-9a-f]*$/i.test(value)) return "notHex";
  if (value.length < MIN_HASH_LENGTH) return "short";
  return null;
}

/** The query to send, or `null` when there is nothing to search. */
export function queryOf(search: RepoSearch): SearchQuery | null {
  const text = search.text.trim();
  if (!search.open || text === "") return null;
  if (search.field === "hash" && hashProblem(text)) return null;
  // A regular expression only applies to the message and the author.
  const regex =
    search.regex && (search.field === "message" || search.field === "author");
  return { text, field: search.field, regex, matchCase: search.matchCase };
}

/** Each run has its own id; batches of older runs are ignored. */
let nextRun = 0;
const runs: Record<number, number> = {};

export const useSearchStore = create<SearchState>()((set, get) => {
  const update = (repoId: number, change: Partial<RepoSearch>) =>
    set((s) => ({
      searches: {
        ...s.searches,
        [repoId]: { ...(s.searches[repoId] ?? fresh()), ...change },
      },
    }));

  const cleared = (): Partial<RepoSearch> => ({
    status: "idle",
    matches: new Set(),
    version: 0,
    found: 0,
    examined: 0,
    pending: null,
    outside: null,
    error: null,
  });

  return {
    ...initialSearchState,

    openBar: (repoId) => {
      const search = get().searches[repoId] ?? fresh();
      update(repoId, { open: true, focusRequest: search.focusRequest + 1 });
    },

    close: (repoId) => {
      get().stop(repoId);
      update(repoId, { open: false, ...cleared() });
    },

    setQuery: (repoId, query) => update(repoId, query),

    run: async (repoId) => {
      const search = get().searches[repoId];
      const query = search ? queryOf(search) : null;
      const run = ++nextRun;
      runs[repoId] = run;
      if (!query) {
        update(repoId, cleared());
        return;
      }
      const matches = new Set<string>();
      update(repoId, {
        ...cleared(),
        status: "running",
        matches,
        progressKnown:
          query.field === "hash" || (!query.regex && query.field !== "path"),
      });
      const result = await searchCommits(repoId, query, (batch) => {
        if (runs[repoId] !== run) return;
        for (const oid of batch.oids) matches.add(oid);
        const now = get().searches[repoId];
        update(repoId, {
          version: (now?.version ?? 0) + 1,
          found: batch.found,
          examined: batch.examined,
        });
      });
      if (runs[repoId] !== run) return;
      if (result.status === "error") {
        // Cancelled: stopped on purpose (a newer run, or another tab).
        if (result.error.kind === "cancelled") return;
        update(repoId, { status: "error", error: result.error });
        return;
      }
      const now = get().searches[repoId];
      update(repoId, {
        status: "done",
        found: result.data.found,
        examined: result.data.examined,
        outside: result.data.outside,
        version: (now?.version ?? 0) + 1,
      });
    },

    stop: (repoId) => {
      runs[repoId] = ++nextRun;
      const search = get().searches[repoId];
      if (search?.status === "running") {
        void commands.cancelSearch(repoId);
        update(repoId, { status: "idle" });
      }
    },

    setPending: (repoId, pending) => update(repoId, { pending }),

    forget: (repoId) => {
      get().stop(repoId);
      set((s) => {
        const searches = { ...s.searches };
        delete searches[repoId];
        return { searches };
      });
    },
  };
});

/** The search of `repoId`, or the closed default. */
export function searchOf(
  searches: Record<number, RepoSearch>,
  repoId: number,
): RepoSearch {
  return searches[repoId] ?? CLOSED;
}

const CLOSED = fresh();
