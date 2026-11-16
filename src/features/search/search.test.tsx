import { act, fireEvent, render, screen, within } from "@testing-library/react";
import i18n from "../../i18n";
import type { GraphLine, SearchBatch, SearchSummary } from "../../ipc";
import {
  commandError,
  ipc,
  sampleLines,
  sampleRefs,
  streamEvents,
} from "../../test/ipc";
import { drawGraph, DIMMED_NODE_ALPHA } from "../graph/drawGraph";
import { GraphView } from "../graph/GraphView";
import { useGraphStore } from "../graph/store";
import { loadedMatches, nextMatch, positionOf } from "./navigate";
import { SEARCH_DELAY_MS } from "./SearchBar";
import { useSearchStore } from "./store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

const lines = sampleLines();
const oids = lines.map((l) => l.oid);

/** The backend finds `found`, in batches of one, then ends. */
function finds(found: string[], outside: string | null = null) {
  ipc.search.mockImplementation(async (_id, _query, onBatch) => {
    found.forEach((oid, i) =>
      onBatch({ oids: [oid], found: i + 1, examined: 0 } as SearchBatch),
    );
    return {
      status: "ok",
      data: { found: found.length, examined: 0, outside } as SearchSummary,
    };
  });
}

async function renderGraph(page: GraphLine[] = lines) {
  ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
  streamEvents([
    { type: "page", lines: page },
    { type: "done", total: page.length, suggestOptimize: false },
  ]);
  const view = render(<GraphView repoId={1} info={info} />);
  await screen.findByRole("listbox");
  await within(screen.getByRole("listbox")).findAllByRole("option");
  return view;
}

const press = (key: string, extra: Partial<KeyboardEventInit> = {}) =>
  fireEvent.keyDown(document.body, { key, ...extra });

/** Opens the bar with ⌘F and searches `text` (after the pause). */
async function searchFor(text: string) {
  press("f", { metaKey: true });
  const field = screen.getByRole("searchbox", { name: "Texto a buscar" });
  fireEvent.change(field, { target: { value: text } });
  await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
  return field;
}

const rows = () => within(screen.getByRole("listbox")).getAllByRole("option");
const selected = () => useGraphStore.getState().graphs[1]?.selectedOid;

beforeEach(async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  await i18n.changeLanguage("es");
});

afterEach(() => {
  vi.useRealTimers();
});

describe("opening the search", () => {
  it("⌘F opens it with the focus in the field", async () => {
    await renderGraph();
    expect(screen.queryByRole("search")).toBeNull();
    press("f", { metaKey: true });
    const field = screen.getByRole("searchbox", { name: "Texto a buscar" });
    expect(field).toHaveFocus();
    expect(
      screen.getByRole("search", { name: "Buscar en el historial" }),
    ).toBeVisible();
  });

  it("only while the history is on screen", async () => {
    ipc.commitFileDiff.mockReturnValue(new Promise(() => {}));
    ipc.conflictFile.mockReturnValue(new Promise(() => {}));
    await renderGraph();
    // A diff over the history.
    act(() =>
      useGraphStore.getState().openFileDiff(1, {
        kind: "commit",
        oid: oids[0]!,
        path: "a.txt",
        oldPath: null,
      }),
    );
    press("f", { metaKey: true });
    expect(screen.queryByRole("search")).toBeNull();
    // The conflict view.
    act(() =>
      useGraphStore
        .getState()
        .openFileDiff(1, { kind: "conflict", path: "a.txt" }),
    );
    press("f", { metaKey: true });
    expect(screen.queryByRole("search")).toBeNull();
    act(() => useGraphStore.getState().closeFile(1));

    // A dialog open over everything.
    const dialog = document.createElement("div");
    dialog.setAttribute("aria-modal", "true");
    document.body.append(dialog);
    press("f", { metaKey: true });
    expect(screen.queryByRole("search")).toBeNull();
    dialog.remove();

    press("f", { metaKey: true });
    expect(screen.getByRole("search")).toBeVisible();
  });

  it("is Ctrl+F on Windows and Linux", async () => {
    const { setPlatformForTests } = await import("../../app/platform");
    setPlatformForTests("windows");
    await renderGraph();
    press("f", { metaKey: true });
    expect(screen.queryByRole("search")).toBeNull();
    press("f", { ctrlKey: true });
    expect(screen.getByRole("search")).toBeVisible();
  });
});

describe("searching", () => {
  it("searches after a pause and fades what does not match", async () => {
    finds([oids[1]!, oids[3]!]);
    await renderGraph();
    press("f", { metaKey: true });
    const field = screen.getByRole("searchbox");
    fireEvent.change(field, { target: { value: "merge" } });
    expect(ipc.search).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
    expect(ipc.search).toHaveBeenCalledTimes(1);
    expect(ipc.search.mock.calls[0]?.[1]).toEqual({
      text: "merge",
      field: "message",
      regex: false,
      matchCase: false,
    });
    expect(screen.getByText("2 resultados")).toBeVisible();
    const [first, second, , fourth] = rows();
    expect(first).toHaveClass("rowDimmed");
    expect(second).not.toHaveClass("rowDimmed");
    expect(fourth).not.toHaveClass("rowDimmed");
  });

  it("says when it is still searching (120+)", async () => {
    let finish: (() => void) | undefined;
    ipc.search.mockImplementation(
      (_id, _query, onBatch) =>
        new Promise((resolve) => {
          onBatch({ oids: [oids[2]!], found: 1, examined: 3 });
          finish = () =>
            resolve({
              status: "ok",
              data: { found: 1, examined: 6, outside: null },
            });
        }),
    );
    await renderGraph();
    await searchFor("c");
    expect(screen.getByText("1+ resultado")).toBeVisible();
    expect(
      screen.getByRole("progressbar", { name: "Progreso de la búsqueda" }),
    ).toHaveAttribute("value", "50");
    expect(screen.getByText("50 %")).toBeVisible();
    await act(async () => finish?.());
    expect(screen.getByText("1 resultado")).toBeVisible();
    expect(screen.queryByRole("progressbar")).toBeNull();
  });

  it("Cancelar stops it", async () => {
    ipc.search.mockImplementation(() => new Promise(() => {}));
    ipc.cancelSearch.mockResolvedValue({ status: "ok", data: null });
    await renderGraph();
    await searchFor("x");
    fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelSearch).toHaveBeenCalledWith(1);
  });

  it("each new query replaces the running search", async () => {
    finds([oids[0]!]);
    await renderGraph();
    const field = await searchFor("a");
    fireEvent.change(field, { target: { value: "ab" } });
    fireEvent.change(field, { target: { value: "abc" } });
    await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
    expect(ipc.search).toHaveBeenCalledTimes(2);
    expect(ipc.search.mock.calls[1]?.[1].text).toBe("abc");
  });
});

describe("moving between results", () => {
  it("Intro goes to the next, older one; ⇧Intro back; it wraps", async () => {
    finds([oids[1]!, oids[3]!]);
    await renderGraph();
    const field = await searchFor("merge");
    fireEvent.keyDown(field, { key: "Enter" });
    expect(selected()).toBe(oids[1]);
    expect(screen.getByText("1 de 2")).toBeVisible();
    fireEvent.keyDown(field, { key: "Enter" });
    expect(selected()).toBe(oids[3]);
    expect(screen.getByText("2 de 2")).toBeVisible();
    fireEvent.keyDown(field, { key: "Enter" });
    expect(selected()).toBe(oids[1]);
    fireEvent.keyDown(field, { key: "Enter", shiftKey: true });
    expect(selected()).toBe(oids[3]);
  });

  it("⌘G and ⇧⌘G work outside the field; the selected row is never faded", async () => {
    finds([oids[1]!, oids[3]!]);
    await renderGraph();
    await searchFor("merge");
    // From a commit picked by hand, the next result below it.
    act(() => useGraphStore.getState().select(1, oids[2]!));
    expect(rows()[2]).not.toHaveClass("rowDimmed");
    press("g", { metaKey: true });
    expect(selected()).toBe(oids[3]);
    press("g", { metaKey: true, shiftKey: true });
    expect(selected()).toBe(oids[1]);
  });

  it("waits for a result the graph has not loaded yet", async () => {
    finds([oids[5]!]);
    // The graph has only loaded its first rows so far.
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    let more: (() => void) | undefined;
    ipc.streamGraph.mockImplementation(
      (_id, onEvent) =>
        new Promise((resolve) => {
          onEvent({ type: "page", lines: lines.slice(0, 3) });
          more = () => {
            onEvent({ type: "page", lines: lines.slice(3) });
            onEvent({ type: "done", total: 6, suggestOptimize: false });
            resolve({ status: "ok", data: null });
          };
        }),
    );
    render(<GraphView repoId={1} info={info} />);
    await within(await screen.findByRole("listbox")).findAllByRole("option");
    const field = await searchFor("old");
    fireEvent.keyDown(field, { key: "Enter" });
    expect(selected()).toBeNull();
    await act(async () => more?.());
    expect(selected()).toBe(oids[5]);
  });
});

describe("what each field allows", () => {
  it("regular expressions only for the message and the author", async () => {
    await renderGraph();
    press("f", { metaKey: true });
    const regex = () =>
      screen.getByRole("checkbox", { name: "Expresión regular" });
    expect(regex()).toBeEnabled();
    fireEvent.change(screen.getByRole("combobox", { name: "Buscar en" }), {
      target: { value: "path" },
    });
    expect(regex()).toBeDisabled();
    expect(regex()).toHaveAccessibleDescription(
      "Solo para el mensaje y el autor",
    );
    fireEvent.change(screen.getByRole("combobox", { name: "Buscar en" }), {
      target: { value: "hash" },
    });
    expect(regex()).toBeDisabled();
  });

  it("a hash needs 4 hex characters before searching", async () => {
    await renderGraph();
    press("f", { metaKey: true });
    fireEvent.change(screen.getByRole("combobox", { name: "Buscar en" }), {
      target: { value: "hash" },
    });
    const field = screen.getByRole("searchbox");
    fireEvent.change(field, { target: { value: "ab" } });
    await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
    expect(field).toHaveAccessibleDescription(
      "Escribe al menos 4 caracteres del hash.",
    );
    fireEvent.change(field, { target: { value: "xyz1" } });
    expect(field).toHaveAccessibleDescription(
      "Un hash solo tiene números y letras de la a a la f.",
    );
    await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
    expect(ipc.search).not.toHaveBeenCalled();
  });

  it("a commit outside every branch can get a branch", async () => {
    finds([], "9".repeat(40));
    await renderGraph();
    await searchFor("9999999");
    expect(
      screen.getByText(/El commit 9999999 no está en ninguna rama/),
    ).toBeVisible();
  });

  it("explains a regular expression git cannot read", async () => {
    ipc.search.mockResolvedValue({
      status: "error",
      error: commandError(
        "invalidPattern",
        "$ git log …\nfatal: command line, 'a(': parentheses not balanced\n(exit code 128)",
      ),
    });
    await renderGraph();
    const field = await searchFor("a(");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Git no entiende esa expresión regular: command line, 'a(': parentheses not balanced.",
    );
  });
});

describe("closing and tabs", () => {
  it("Esc closes it and nothing stays faded", async () => {
    finds([oids[1]!]);
    await renderGraph();
    const field = await searchFor("merge");
    expect(rows()[0]).toHaveClass("rowDimmed");
    fireEvent.keyDown(field, { key: "Escape" });
    expect(screen.queryByRole("search")).toBeNull();
    expect(rows()[0]).not.toHaveClass("rowDimmed");
  });

  it("each tab keeps its search; leaving stops it and coming back runs it again", async () => {
    ipc.search.mockImplementation(() => new Promise(() => {}));
    ipc.cancelSearch.mockResolvedValue({ status: "ok", data: null });
    const view = await renderGraph();
    await searchFor("merge");
    expect(ipc.search).toHaveBeenCalledTimes(1);
    view.unmount();
    expect(ipc.cancelSearch).toHaveBeenCalledWith(1);

    finds([oids[1]!]);
    render(<GraphView repoId={1} info={info} />);
    expect(screen.getByRole("searchbox")).toHaveValue("merge");
    await act(() => vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS + 10));
    expect(ipc.search).toHaveBeenCalledTimes(2);
    expect(screen.getByText("1 resultado")).toBeVisible();
    expect(useSearchStore.getState().searches[1]?.open).toBe(true);
  });
});

describe("helpers", () => {
  const set = new Set([oids[1]!, oids[4]!]);

  it("find the next and previous result in the graph's order", () => {
    expect(nextMatch(lines, set, -1, "next")).toBe(1);
    expect(nextMatch(lines, set, 1, "next")).toBe(4);
    expect(nextMatch(lines, set, 4, "next")).toBe(-1);
    expect(nextMatch(lines, set, 4, "previous")).toBe(1);
    expect(nextMatch(lines, set, lines.length, "previous")).toBe(4);
    expect(loadedMatches(lines, set)).toBe(2);
    expect(positionOf(lines, set, oids[4]!)).toBe(2);
    expect(positionOf(lines, set, oids[0]!)).toBe(0);
  });

  it("draw the nodes of unmatched commits faded, the lines untouched", () => {
    const alphas: number[] = [];
    const ctx = {
      globalAlpha: 1,
      setTransform: () => {},
      clearRect: () => {},
      beginPath: () => {},
      moveTo: () => {},
      lineTo: () => {},
      bezierCurveTo: () => {},
      arc: () => {},
      fill() {
        alphas.push(this.globalAlpha);
      },
      stroke() {},
    };
    drawGraph(ctx as unknown as CanvasRenderingContext2D, {
      lines,
      scrollTop: 0,
      width: 100,
      height: 600,
      pixelRatio: 1,
      headOid: null,
      selectedOid: oids[0]!,
      isDimmed: (oid) => !set.has(oid),
      background: "#000",
      ringColor: "#fff",
      laneColors: ["#f00"],
    });
    // Commit 0 is selected: never faded. 1 and 4 match.
    const filled = alphas.filter((a) => a === DIMMED_NODE_ALPHA).length;
    expect(filled).toBeGreaterThan(0);
    expect(alphas[0]).toBe(1);
    expect(ctx.globalAlpha).toBe(1);
  });
});
