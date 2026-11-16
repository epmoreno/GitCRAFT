import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type {
  BlameChunk,
  BlameCommit,
  BlameOutcome,
  BlameStream,
} from "../../ipc";
import { cleanStatus, ipc, oid } from "../../test/ipc";
import { useGraphStore, type OpenFile, type RepoGraph } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";
import { BlameView } from "./BlameView";

const now = Math.floor(Date.now() / 1000);

const commit = (label: string, author: string, summary: string) =>
  ({
    oid: oid(label),
    authorName: author,
    authorEmail: `${author.toLowerCase()}@example.invalid`,
    authorTime: now - 7200,
    summary,
    boundary: label === "a",
  }) satisfies BlameCommit;

const chunk = (
  label: string,
  line: number,
  count: number,
  previous: string | null = "a",
): BlameChunk => ({
  oid: oid(label),
  line,
  count,
  origLine: line,
  path: "guía.txt",
  previous: previous
    ? { oid: oid(previous), path: "notas antiguas.txt" }
    : null,
});

const TEXT = ["uno", "dos", "tres", "cuatro", "cinco"];

/** A blame of five lines: A has 1, 3–5 (with nothing before), B has 2. */
const events: BlameStream[] = [
  { type: "text", lines: TEXT },
  {
    type: "batch",
    commits: [commit("b", "Luis", "Cambia la dos")],
    chunks: [chunk("b", 2, 1)],
    attributed: 1,
  },
  {
    type: "batch",
    commits: [commit("a", "Ana", "Crea la guía")],
    chunks: [chunk("a", 1, 1, null), chunk("a", 3, 3, null)],
    attributed: 5,
  },
];

function sends(
  stream: BlameStream[],
  outcome: BlameOutcome = { type: "done", lines: 5 },
) {
  ipc.blame.mockImplementation(async (_id, _path, _rev, _force, onEvent) => {
    for (const event of stream) onEvent(event);
    return { status: "ok", data: outcome };
  });
}

function graphWith(openFile: OpenFile | null) {
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
      } satisfies RepoGraph,
    },
  });
}

/** Renders the open blame the way GraphView does. */
function View() {
  const open = useGraphStore((s) => s.graphs[1]?.openFile);
  if (open?.kind !== "blame") return null;
  return (
    <BlameView
      key={`${open.rev}:${open.path}`}
      repoId={1}
      path={open.path}
      rev={open.rev}
      chosen={open.chosen}
      canGoBack={open.back.length > 0}
    />
  );
}

const openFile = () => useGraphStore.getState().graphs[1]?.openFile;

async function openBlame(rev = "HEAD", path = "guía.txt") {
  graphWith({ kind: "blame", path, rev, chosen: null, back: [] });
  render(<View />);
  return screen.findByRole("region", { name: `Blame de ${path}` });
}

const rows = (view: HTMLElement) =>
  within(within(view).getByRole("table")).getAllByRole("row");

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
  ipc.cancelBlame.mockResolvedValue({ status: "ok", data: null });
});

describe("the lines", () => {
  it("shows each line with its commit on the first line of each group", async () => {
    sends(events);
    const view = await openBlame();
    expect(ipc.blame).toHaveBeenCalledWith(
      1,
      "guía.txt",
      "HEAD",
      false,
      expect.any(Function),
    );
    const lines = rows(view);
    expect(lines).toHaveLength(5);
    expect(lines[0]).toHaveTextContent("aaaaaaa Ana hace 2 horas");
    expect(lines[0]).toHaveTextContent("1uno");
    expect(lines[1]).toHaveTextContent("bbbbbbb Luis");
    expect(lines[2]).toHaveTextContent("aaaaaaa Ana");
    // The rest of a group has no column text.
    expect(lines[3]).not.toHaveTextContent("Ana");
    // Neighbouring groups alternate.
    expect(lines[0]).not.toHaveClass("band");
    expect(lines[1]).toHaveClass("band");
    expect(lines[2]).not.toHaveClass("band");
    expect(lines[4]).not.toHaveClass("band");
  });

  it("shows lines not attributed yet in grey, and the progress", async () => {
    ipc.blame.mockImplementation(async (_id, _p, _r, _f, onEvent) => {
      onEvent(events[0]!);
      onEvent(events[1]!);
      return new Promise(() => {});
    });
    const view = await openBlame();
    const lines = rows(view);
    expect(lines[0]).toHaveClass("pending");
    expect(lines[1]).not.toHaveClass("pending");
    expect(
      within(view).getByRole("progressbar", { name: "Progreso del blame" }),
    ).toHaveAttribute("value", "1");
    expect(view).toHaveTextContent("1 de 5 líneas (20 %)");
    fireEvent.click(within(view).getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelBlame).toHaveBeenCalledWith(1);
  });

  it("a click picks the commit: its lines are marked", async () => {
    sends(events);
    const view = await openBlame();
    fireEvent.click(within(view).getByRole("button", { name: /líneas 1 a 1/ }));
    expect(openFile()).toMatchObject({ kind: "blame", chosen: oid("a") });
    const lines = rows(view);
    expect(lines[0]).toHaveClass("chosen");
    expect(lines[1]).not.toHaveClass("chosen");
    expect(lines[4]).toHaveClass("chosen");
  });

  it("moves between groups with the arrows", async () => {
    sends(events);
    const view = await openBlame();
    const first = within(view).getByRole("button", {
      name: /líneas 1 a 1/,
    });
    fireEvent.keyDown(first, { key: "ArrowDown" });
    expect(openFile()).toMatchObject({ chosen: oid("b") });
    const second = within(view).getByRole("button", {
      name: /líneas 2 a 2/,
    });
    fireEvent.keyDown(second, { key: "ArrowDown" });
    expect(openFile()).toMatchObject({ chosen: oid("a") });
  });
});

describe("a group's menu", () => {
  it("blames before the commit and goes back with the button and ⌘[", async () => {
    sends(events);
    const view = await openBlame();
    const second = within(view).getByRole("button", { name: /líneas 2 a 2/ });
    fireEvent.keyDown(second, { key: "F10", shiftKey: true });
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Blame antes de este commit",
      }),
    );
    expect(openFile()).toEqual({
      kind: "blame",
      path: "notas antiguas.txt",
      rev: oid("a"),
      chosen: null,
      back: [{ path: "guía.txt", rev: "HEAD" }],
    });
    expect(ipc.blame).toHaveBeenLastCalledWith(
      1,
      "notas antiguas.txt",
      oid("a"),
      false,
      expect.any(Function),
    );

    // Back with ⌘[ …
    fireEvent.keyDown(window, { key: "[", metaKey: true });
    expect(openFile()).toMatchObject({ path: "guía.txt", back: [] });

    // … and with the button (also ⌥⌘[ on keyboards that need ⌥ for "[").
    await user.click(
      within(screen.getByRole("region")).getByRole("button", {
        name: /líneas 2 a 2/,
      }),
    );
    fireEvent.contextMenu(
      within(screen.getByRole("region")).getByRole("button", {
        name: /líneas 2 a 2/,
      }),
    );
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Blame antes de este commit",
      }),
    );
    await user.click(screen.getByRole("button", { name: /Blame anterior/ }));
    expect(openFile()).toMatchObject({ path: "guía.txt", back: [] });
    expect(screen.queryByRole("button", { name: /Blame anterior/ })).toBeNull();
  });

  it("is Alt+← on Windows and Linux", async () => {
    const { setPlatformForTests } = await import("../../app/platform");
    setPlatformForTests("windows");
    sends(events);
    graphWith({
      kind: "blame",
      path: "guía.txt",
      rev: oid("a"),
      chosen: null,
      back: [{ path: "guía.txt", rev: "HEAD" }],
    });
    render(<View />);
    await screen.findByRole("region");
    fireEvent.keyDown(window, { key: "[", ctrlKey: true });
    expect(openFile()).toMatchObject({ rev: oid("a") });
    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    expect(openFile()).toMatchObject({ rev: "HEAD", back: [] });
  });

  it("nothing before the file's first commit", async () => {
    sends(events);
    const view = await openBlame();
    fireEvent.contextMenu(
      within(view).getByRole("button", { name: /líneas 1 a 1/ }),
    );
    const item = within(screen.getByRole("menu")).getByRole("menuitem", {
      name: "Blame antes de este commit",
    });
    expect(item).toBeDisabled();
    expect(item).toHaveAccessibleDescription(
      "Es el primer commit de este archivo",
    );
  });

  it("shows the commit in the graph, or the file's history", async () => {
    sends(events);
    const view = await openBlame();
    fireEvent.contextMenu(
      within(view).getByRole("button", { name: /líneas 2 a 2/ }),
    );
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Historial del archivo",
      }),
    );
    expect(openFile()).toEqual({
      kind: "history",
      path: "guía.txt",
      rev: oid("b"),
      chosen: null,
    });

    act(() =>
      graphWith({
        kind: "blame",
        path: "guía.txt",
        rev: "HEAD",
        chosen: null,
        back: [],
      }),
    );
    const again = await screen.findByRole("region");
    fireEvent.contextMenu(
      within(again).getByRole("button", { name: /líneas 2 a 2/ }),
    );
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Ver el commit en el grafo",
      }),
    );
    expect(openFile()).toBeNull();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("b"));
  });
});

describe("files it cannot blame at once", () => {
  it("asks before a big file", async () => {
    sends([], { type: "large", lines: 34567 });
    const view = await openBlame();
    expect(
      await within(view).findByText(
        "Este archivo tiene 34.567 líneas: el blame puede tardar.",
      ),
    ).toBeVisible();
    sends(events);
    fireEvent.click(
      within(view).getByRole("button", { name: "Hacer el blame" }),
    );
    expect(ipc.blame).toHaveBeenLastCalledWith(
      1,
      "guía.txt",
      "HEAD",
      true,
      expect.any(Function),
    );
    expect(await within(view).findAllByRole("row")).toHaveLength(5);
  });

  it.each([
    [{ type: "binary" }, "Es un archivo binario"],
    [{ type: "missing" }, "El archivo no existe en ese commit."],
    [{ type: "tooLarge", bytes: 23 * 1024 * 1024 }, "Pesa 23 MB"],
  ] as [BlameOutcome, string][])("explains %o", async (outcome, text) => {
    sends([], outcome);
    const view = await openBlame();
    expect(await within(view).findByText(new RegExp(text))).toBeVisible();
  });
});

describe("where it opens and how it closes", () => {
  it("Esc goes back to the graph, which keeps its selection", async () => {
    sends(events);
    await openBlame();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(openFile()).toBeNull();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("e"));
    expect(ipc.cancelBlame).toHaveBeenCalledWith(1);
  });

  it("from a file history row; not where the file was deleted", async () => {
    const { FileHistoryView } = await import("../fileHistory/FileHistoryView");
    ipc.fileHistory.mockImplementation(async (_id, _p, _r, onBatch) => {
      onBatch([
        {
          oid: oid("c"),
          authorName: "Ana",
          authorEmail: "a@example.invalid",
          authorTime: now,
          summary: "Borra",
          path: "guía.txt",
          oldPath: null,
          status: "deleted",
        },
        {
          oid: oid("b"),
          authorName: "Ana",
          authorEmail: "a@example.invalid",
          authorTime: now,
          summary: "Cambia",
          path: "guía.txt",
          oldPath: null,
          status: "modified",
        },
      ]);
      return { status: "ok", data: { count: 2 } };
    });
    ipc.commitFileDiff.mockReturnValue(new Promise(() => {}));
    graphWith({ kind: "history", path: "guía.txt", rev: "HEAD", chosen: null });
    render(
      <FileHistoryView repoId={1} path="guía.txt" rev="HEAD" chosen={null} />,
    );
    const options = await screen.findAllByRole("option");
    fireEvent.contextMenu(options[0]!);
    expect(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Blame en este commit",
      }),
    ).toBeDisabled();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    fireEvent.contextMenu(options[1]!);
    const user = userEvent.setup();
    await user.click(
      within(screen.getByRole("menu")).getByRole("menuitem", {
        name: "Blame en este commit",
      }),
    );
    expect(openFile()).toEqual({
      kind: "blame",
      path: "guía.txt",
      rev: oid("b"),
      chosen: null,
      back: [],
    });
  });

  it("the column's width changes with the keyboard", async () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      function (this: HTMLElement) {
        const width = this.tagName === "SECTION" ? 1200 : 260;
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
    sends(events);
    const view = await openBlame();
    const divider = within(view).getByRole("separator", {
      name: "Anchura de la columna del blame",
    });
    fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(divider).toHaveAttribute("aria-valuenow", "276");
    expect(view.style.getPropertyValue("--blame-gutter")).toBe("276px");
  });
});
