import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import {
  commandError,
  ipc,
  oid,
  sampleLines,
  sampleRefs,
  streamEvents,
} from "../../test/ipc";
import { GraphView } from "./GraphView";
import { useGraphStore } from "./store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

function renderGraph() {
  return render(<GraphView repoId={1} info={info} />);
}

describe("GraphView", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("shows each commit with labels, author, relative date and short hash", async () => {
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
    renderGraph();

    const list = await screen.findByRole("listbox", {
      name: "Historial de sample",
    });
    const rows = await within(list).findAllByRole("option");
    expect(rows).toHaveLength(6);

    const tip = rows[0]!;
    expect(within(tip).getByText("E")).toHaveClass("selectable");
    expect(await within(tip).findByText("main")).toHaveAttribute(
      "title",
      "Rama local main",
    );
    expect(within(tip).getByText("origin/main")).toBeVisible();
    expect(within(tip).getByText("Ana")).toBeVisible();
    expect(within(tip).getByText("hace 3 días")).toBeVisible();
    const sha = within(tip).getByText(oid("e").slice(0, 7));
    expect(sha).toHaveClass("selectable");
    expect(sha).toHaveAttribute("title", oid("e"));

    expect(within(rows[4]!).getByText("v1.0")).toBeVisible();
    expect(screen.getByText("6 commits")).toBeVisible();
    expect(screen.getByText("/src/sample")).toHaveClass("selectable");
  });

  it("selects a commit on click", async () => {
    const user = userEvent.setup();
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
    renderGraph();
    const rows = await screen.findAllByRole("option");
    await user.click(within(rows[2]!).getByText(oid("d").slice(0, 7)));
    expect(rows[2]).toHaveAttribute("aria-selected", "true");
    expect(rows[0]).toHaveAttribute("aria-selected", "false");
    // The history takes the focus, so the selection shows as active.
    expect(screen.getByRole("listbox")).toHaveFocus();
  });

  it("reflects a selection made elsewhere (e.g. the sidebar)", async () => {
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
    renderGraph();
    const rows = await screen.findAllByRole("option");
    act(() => useGraphStore.getState().select(1, oid("b")));
    expect(rows[4]).toHaveAttribute("aria-selected", "true");
  });

  it("shows progress while loading", async () => {
    ipc.streamGraph.mockImplementation(async (_id, onEvent) => {
      onEvent({ type: "page", lines: sampleLines().slice(0, 2) });
      return new Promise(() => {}); // never finishes
    });
    renderGraph();
    expect(await screen.findByText("Cargando… 2 commits")).toBeVisible();
  });

  it("explains an empty repository", async () => {
    streamEvents([{ type: "done", total: 0, suggestOptimize: false }]);
    renderGraph();
    expect(
      await screen.findByText("Este repositorio todavía no tiene commits."),
    ).toBeVisible();
    expect(screen.getByText("0 commits")).toBeVisible();
  });

  it("shows load errors with git's output", async () => {
    ipc.streamGraph.mockResolvedValue({
      status: "error",
      error: commandError("unknown", "$ git log\nfatal: bad object"),
    });
    renderGraph();
    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText(/No se pudo cargar el historial/),
    ).toBeVisible();
    expect(within(alert).getByText(/fatal: bad object/)).toHaveClass(
      "selectable",
    );
  });

  it("loads the graph only once per repository", async () => {
    streamEvents([{ type: "done", total: 0, suggestOptimize: false }]);
    const { unmount } = renderGraph();
    await screen.findByText("0 commits");
    unmount();
    renderGraph();
    await screen.findByText("0 commits");
    expect(ipc.streamGraph).toHaveBeenCalledTimes(1);
  });
});
