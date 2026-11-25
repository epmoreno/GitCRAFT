import { cleanStatus, ipc } from "../../test/ipc";
import { useGraphStore, type OpenFile } from "../graph/store";
import { useWorkdirStore } from "./store";

const file = (overrides: Partial<OpenFile> = {}) =>
  ({
    kind: "worktree",
    area: "untracked",
    path: "nuevo.txt",
    oldPath: null,
    ...overrides,
  }) as OpenFile;

function open(openFile: OpenFile) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: null,
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile,
        loadId: 1,
      },
    },
  });
}
const opened = () => useGraphStore.getState().graphs[1]?.openFile;

async function refreshTo(
  lists: Partial<ReturnType<typeof cleanStatus>>,
): Promise<void> {
  ipc.workingStatus.mockResolvedValue({
    status: "ok",
    data: { ...cleanStatus(), ...lists },
  });
  await useWorkdirStore.getState().refresh(1);
}
const entry = (status: "untracked" | "added" | "modified") => ({
  path: "nuevo.txt",
  oldPath: null,
  status,
});

describe("the open uncommitted change", () => {
  it("goes back to the history when its file is no longer a change", async () => {
    open(file());
    await refreshTo({ untracked: [entry("untracked")] });
    expect(opened()).toEqual(file());
    // Discarded: it went to the Trash.
    await refreshTo({});
    expect(opened()).toBeNull();
  });

  it("follows the file when it is staged or unstaged whole", async () => {
    open(file());
    await refreshTo({ staged: [entry("added")] });
    expect(opened()).toEqual(file({ area: "staged" }));
    await refreshTo({ untracked: [entry("untracked")] });
    expect(opened()).toEqual(file());
  });

  it("stays in its list while part of the change is still there", async () => {
    open(file({ area: "unstaged" }));
    await refreshTo({
      staged: [entry("modified")],
      unstaged: [entry("modified")],
    });
    expect(opened()).toEqual(file({ area: "unstaged" }));
  });

  it("leaves the diff of a commit alone, and any diff if the status fails", async () => {
    const commit = {
      kind: "commit",
      oid: "a".repeat(40),
      path: "nuevo.txt",
      oldPath: null,
    } as const;
    open(commit);
    await refreshTo({});
    expect(opened()).toEqual(commit);

    open(file());
    ipc.workingStatus.mockResolvedValue({
      status: "error",
      error: { kind: "unknown", message: "", details: null, source: "git" },
    });
    await useWorkdirStore.getState().refresh(1);
    expect(opened()).toEqual(file());
  });
});
