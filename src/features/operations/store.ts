import { create } from "zustand";
import { commands, type CommandRecord } from "../../ipc";

/** Records kept, like the backend's history. */
export const HISTORY_LIMIT = 1000;

type OperationsState = {
  open: boolean;
  /** Oldest first. */
  records: CommandRecord[];
  /** Show only the commands of the active tab's repository. */
  onlyActive: boolean;
  /** Hide commands that only read the repository. */
  hideReads: boolean;
  toggle: () => void;
  close: () => void;
  /** Loads the history kept by the backend. */
  load: () => Promise<void>;
  /** A command started or finished. */
  receive: (record: CommandRecord) => void;
  clear: () => Promise<void>;
  setOnlyActive: (on: boolean) => void;
  setHideReads: (on: boolean) => void;
};

export const initialOperationsState = {
  open: false,
  records: [],
  onlyActive: true,
  hideReads: false,
} satisfies Partial<OperationsState>;

/** `records` with `incoming` added or updated (by id), oldest first. */
function merge(records: CommandRecord[], incoming: CommandRecord[]) {
  const byId = new Map(records.map((r) => [r.id, r]));
  for (const record of incoming) byId.set(record.id, record);
  return [...byId.values()]
    .sort((a, b) => a.startedAt - b.startedAt || a.id - b.id)
    .slice(-HISTORY_LIMIT);
}

export const useOperationsStore = create<OperationsState>()((set) => ({
  ...initialOperationsState,

  toggle: () => set((s) => ({ open: !s.open })),
  close: () => set({ open: false }),

  load: async () => {
    const result = await commands.commandHistory();
    // Events received meanwhile are newer than the loaded copy.
    if (result.status === "ok") {
      set((s) => ({ records: merge(result.data, s.records) }));
    }
  },

  receive: (record) => set((s) => ({ records: merge(s.records, [record]) })),

  clear: async () => {
    await commands.clearCommandHistory();
    set((s) => ({ records: s.records.filter((r) => !r.outcome) }));
  },

  setOnlyActive: (onlyActive) => set({ onlyActive }),
  setHideReads: (hideReads) => set({ hideReads }),
}));

/** Whether `record` ran in the repository at `path` (or inside it). */
export function ranIn(record: CommandRecord, path: string) {
  const root = path.replace(/\/$/, "");
  // No folder: about no repository in particular (gh's session), so it
  // belongs to every tab.
  if (record.cwd === "") return true;
  return record.cwd === root || record.cwd.startsWith(`${root}/`);
}
