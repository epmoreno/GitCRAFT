import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { FileHistoryEntry, WorkingStatus } from "../../ipc";
import { cleanStatus, ipc, oid, textDiff } from "../../test/ipc";
import { DetailPanel } from "../detail/DetailPanel";
import { useGraphStore, WORKTREE, type RepoGraph } from "../graph/store";
import { WorkingDirPanel } from "../workdir/WorkingDirPanel";
import { useWorkdirStore } from "../workdir/store";
import { FileHistoryView } from "./FileHistoryView";

const now = Math.floor(Date.now() / 1000);

function entry(
  label: string,
  summary: string,
  overrides: Partial<FileHistoryEntry> = {},
): FileHistoryEntry {
  return {
    oid: oid(label),
    authorName: "Ana",
    authorEmail: "ana@example.invalid",
    authorTime: now - 3600,
    summary,
    path: "docs/guía.txt",
    oldPath: null,
    status: "modified",
    ...overrides,
  };
}

const history = [
  entry("c", "Amplía la guía"),
  entry("b", "Mueve las notas", {
    status: "renamed",
    oldPath: "notas.txt",
  }),
  entry("a", "Crea las notas", { status: "added", path: "notas.txt" }),
];

function graphWith(openFile: RepoGraph["openFile"]) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: null,
        error: null,
        selectedOid: oid("e"),
        suggestOptimize: false,
        openFile,
        loadId: 1,
      },
    },
  });
}

function withStatus(change: Partial<WorkingStatus> = {}) {
  useWorkdirStore.setState({
    statuses: { 1: { status: { ...cleanStatus(), ...change }, error: null } },
  });
}

/** The backend sends `entries` (in one batch) and ends. */
function sends(entries: FileHistoryEntry[]) {
  ipc.fileHistory.mockImplementation(async (_id, _path, _rev, onBatch) => {
    if (entries.length > 0) onBatch(entries);
    return { status: "ok", data: { count: entries.length } };
  });
}

/** Renders the open history the way GraphView does. */
function View() {
  const open = useGraphStore((s) => s.graphs[1]?.openFile);
  if (open?.kind !== "history") return null;
  return (
    <FileHistoryView
      repoId={1}
      path={open.path}
      rev={open.rev}
      chosen={open.chosen}
    />
  );
}

const openFile = () => useGraphStore.getState().graphs[1]?.openFile;
const chosen = () => {
  const f = openFile();
  return f?.kind === "history" ? f.chosen : undefined;
};

async function openHistory(rev = "HEAD", path = "docs/guía.txt") {
  graphWith({ kind: "history", path, rev, chosen: null });
  render(<View />);
  return screen.findByRole("region", { name: `Historial de ${path}` });
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  withStatus();
  ipc.commitFileDiff.mockResolvedValue({ status: "ok", data: textDiff() });
  ipc.worktreeFileDiff.mockResolvedValue({ status: "ok", data: textDiff() });
  ipc.cancelFileHistory.mockResolvedValue({ status: "ok", data: null });
});

describe("where it opens", () => {
  it("from a commit's file, by right click or ⇧F10: from that commit", async () => {
    graphWith(null);
    ipc.commitDetails.mockReturnValue(new Promise(() => {}));
    ipc.commitChanges.mockResolvedValue({
      status: "ok",
      data: {
        base: null,
        files: [
          {
            path: "docs/guía.txt",
            oldPath: null,
            status: "modified",
            similarity: null,
            additions: 1,
            deletions: 0,
          },
        ],
      },
    });
    const { FileList } = await import("../detail/FileList");
    render(<FileList repoId={1} oid={oid("c")} isMerge={false} withHistory />);
    const file = await screen.findByRole("button", { name: /guía\.txt/ });
    fireEvent.contextMenu(file);
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Historial del archivo",
      }),
    );
    expect(openFile()).toEqual({
      kind: "history",
      path: "docs/guía.txt",
      rev: oid("c"),
      chosen: null,
    });

    act(() => useGraphStore.getState().closeFile(1));
    file.focus();
    fireEvent.keyDown(file, { key: "F10", shiftKey: true });
    expect(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Ver el diff",
      }),
    ).toBeVisible();
  });

  it("from an uncommitted file: from HEAD", async () => {
    graphWith(null);
    withStatus({
      unstaged: [{ path: "docs/guía.txt", oldPath: null, status: "modified" }],
    });
    render(<WorkingDirPanel repoId={1} />);
    const row = screen
      .getByRole("button", { name: "Ver cambios de docs/guía.txt" })
      .closest("li") as HTMLElement;
    fireEvent.contextMenu(row);
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Historial del archivo",
      }),
    );
    expect(openFile()).toMatchObject({ kind: "history", rev: "HEAD" });
  });
});

describe("the list", () => {
  it("shows each commit and how the file changed; the first is chosen", async () => {
    sends(history);
    const view = await openHistory();
    expect(ipc.fileHistory).toHaveBeenCalledWith(
      1,
      "docs/guía.txt",
      "HEAD",
      expect.any(Function),
    );
    const list = within(view).getByRole("listbox", {
      name: "Commits que cambiaron docs/guía.txt",
    });
    const rows = await within(list).findAllByRole("option");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("Amplía la guía");
    expect(rows[0]).toHaveTextContent("Ana · hace 1 hora · modificado");
    expect(rows[1]).toHaveTextContent("renombrado desde notas.txt");
    // Another path in that commit is shown.
    expect(rows[2]).toHaveTextContent("añadido · notas.txt");
    expect(view).toHaveTextContent("3 commits");
    expect(chosen()).toBe(oid("c"));
    expect(rows[0]).toHaveAttribute("aria-selected", "true");
  });

  it("shows the diff of the chosen commit, with its path then", async () => {
    sends(history);
    const view = await openHistory();
    const rows = await within(view).findAllByRole("option");
    fireEvent.click(rows[1]!);
    expect(chosen()).toBe(oid("b"));
    expect(ipc.commitFileDiff).toHaveBeenLastCalledWith(
      1,
      oid("b"),
      "docs/guía.txt",
      "notas.txt",
      false,
    );
  });

  it("moves with the arrows", async () => {
    sends(history);
    const view = await openHistory();
    const rows = await within(view).findAllByRole("option");
    fireEvent.keyDown(rows[0]!, { key: "ArrowDown" });
    expect(chosen()).toBe(oid("b"));
    fireEvent.keyDown(rows[1]!, { key: "ArrowUp" });
    expect(chosen()).toBe(oid("c"));
  });

  it("the detail panel shows the chosen commit, not the graph's", async () => {
    sends(history);
    ipc.commitDetails.mockReturnValue(new Promise(() => {}));
    await openHistory();
    render(<DetailPanel repoId={1} />);
    await vi.waitFor(() =>
      expect(ipc.commitDetails).toHaveBeenCalledWith(1, oid("c")),
    );
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("e"));
  });

  it("says when the file has no history yet", async () => {
    sends([]);
    const view = await openHistory();
    expect(
      await within(view).findByText("Este archivo aún no tiene historial."),
    ).toBeVisible();
  });

  it("says how many so far while it reads, and can be cancelled", async () => {
    ipc.fileHistory.mockImplementation(async (_id, _path, _rev, onBatch) => {
      onBatch(history.slice(0, 2));
      return new Promise(() => {});
    });
    const view = await openHistory();
    expect(await within(view).findByText("2 commits…")).toBeVisible();
    fireEvent.click(within(view).getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelFileHistory).toHaveBeenCalledWith(1);
  });
});

describe("uncommitted changes", () => {
  it("come first, look different and show staged and unstaged", async () => {
    sends(history);
    withStatus({
      staged: [{ path: "docs/guía.txt", oldPath: null, status: "modified" }],
      unstaged: [{ path: "docs/guía.txt", oldPath: null, status: "modified" }],
    });
    const view = await openHistory();
    const rows = await within(view).findAllByRole("option");
    expect(rows).toHaveLength(4);
    expect(rows[0]).toHaveTextContent("Sin confirmar");
    expect(rows[0]).toHaveTextContent("Preparado · Sin preparar");
    expect(rows[0]).toHaveClass("uncommitted");
    expect(rows[1]).not.toHaveClass("uncommitted");
    expect(chosen()).toBe(WORKTREE);
    expect(ipc.worktreeFileDiff).toHaveBeenCalledWith(
      1,
      "docs/guía.txt",
      null,
      "staged",
      false,
    );
    expect(ipc.worktreeFileDiff).toHaveBeenCalledWith(
      1,
      "docs/guía.txt",
      null,
      "unstaged",
      false,
    );
    expect(
      within(view).getByRole("region", { name: "Preparado" }),
    ).toBeVisible();
    expect(
      within(view).getByRole("region", { name: "Sin preparar" }),
    ).toBeVisible();
  });

  it("are not there when the file has none", async () => {
    sends(history);
    withStatus({
      unstaged: [{ path: "otro.txt", oldPath: null, status: "modified" }],
    });
    const view = await openHistory();
    const rows = await within(view).findAllByRole("option");
    expect(rows).toHaveLength(3);
    expect(view).not.toHaveTextContent("Sin confirmar");
  });

  it("are not there from an older commit", async () => {
    sends(history);
    withStatus({
      unstaged: [{ path: "docs/guía.txt", oldPath: null, status: "modified" }],
    });
    const view = await openHistory(oid("c"));
    const rows = await within(view).findAllByRole("option");
    expect(rows).toHaveLength(3);
  });
});

describe("leaving it", () => {
  it("Esc and «Volver al grafo» go back; the graph keeps its selection", async () => {
    sends(history);
    await openHistory();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(openFile()).toBeNull();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("e"));
    expect(ipc.cancelFileHistory).toHaveBeenCalledWith(1);

    // Opened again (the same view renders it).
    act(() =>
      graphWith({
        kind: "history",
        path: "docs/guía.txt",
        rev: "HEAD",
        chosen: null,
      }),
    );
    const view = await screen.findByRole("region", {
      name: "Historial de docs/guía.txt",
    });
    fireEvent.click(
      within(view).getByRole("button", { name: /Volver al grafo/ }),
    );
    expect(openFile()).toBeNull();
  });

  it("«Ver el commit en el grafo» selects it there; Esc closes the menu first", async () => {
    sends(history);
    const view = await openHistory();
    const rows = await within(view).findAllByRole("option");
    fireEvent.keyDown(rows[0]!, { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu", {
      name: `Commit ${oid("c").slice(0, 7)}`,
    });
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(openFile()?.kind).toBe("history");

    fireEvent.contextMenu(rows[1]!);
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Ver el commit en el grafo",
      }),
    );
    expect(openFile()).toBeNull();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("b"));
  });
});

describe("the divider", () => {
  it("widens and narrows the list with the keyboard", async () => {
    // jsdom has no layout: the view is 1200 px wide, the list 340.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      function (this: HTMLElement) {
        const width = this.tagName === "SECTION" ? 1200 : 340;
        return {
          width,
          height: 600,
          top: 0,
          left: 0,
          right: width,
          bottom: 600,
        } as DOMRect;
      },
    );
    sends(history);
    const view = await openHistory();
    const divider = within(view).getByRole("separator", {
      name: "Anchura de la lista",
    });
    const before = Number(divider.getAttribute("aria-valuenow"));
    fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(Number(divider.getAttribute("aria-valuenow"))).toBe(before + 16);
    fireEvent.keyDown(divider, { key: "Home" });
    expect(divider).toHaveAttribute("aria-valuenow", "200");
    fireEvent.keyDown(divider, { key: "Enter" });
    expect(Number(divider.getAttribute("aria-valuenow"))).toBe(before);
  });
});
