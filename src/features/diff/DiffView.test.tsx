import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import {
  commandError,
  ipc,
  oid,
  sampleLines,
  sampleRefs,
  textDiff,
} from "../../test/ipc";
import { DetailPanel } from "../detail/DetailPanel";
import { GraphView } from "../graph/GraphView";
import { useGraphStore } from "../graph/store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

function withGraph() {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: sampleLines(),
        maxColumn: 1,
        refs: sampleRefs(),
        error: null,
        selectedOid: oid("d"),
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

/** Graph and detail panel side by side, as in the app. */
function renderWorkspace() {
  ipc.commitDetails.mockResolvedValue({
    status: "ok",
    data: {
      oid: oid("d"),
      parents: [oid("c")],
      author: { name: "Ana", email: "ana@example.com", time: 1 },
      committer: { name: "Ana", email: "ana@example.com", time: 1 },
      summary: "D",
      body: "",
    },
  });
  ipc.commitChanges.mockResolvedValue({
    status: "ok",
    data: {
      base: oid("c"),
      files: [
        {
          path: "src/main.rs",
          oldPath: null,
          status: "modified",
          similarity: null,
          additions: 1,
          deletions: 1,
        },
      ],
    },
  });
  withGraph();
  render(
    <>
      <GraphView repoId={1} info={info} />
      <DetailPanel repoId={1} />
    </>,
  );
}

async function openMainRs(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /main\.rs/ }));
  return screen.findByRole("region", { name: "Diff de src/main.rs" });
}

describe("DiffView", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("opens a file's diff with line numbers from the file list", async () => {
    const user = userEvent.setup();
    ipc.commitFileDiff.mockResolvedValue({ status: "ok", data: textDiff() });
    renderWorkspace();

    const view = await openMainRs(user);
    expect(ipc.commitFileDiff).toHaveBeenCalledWith(
      1,
      oid("d"),
      "src/main.rs",
      null,
      false,
    );
    expect(
      await within(view).findByText("@@ -1,3 +1,3 @@ fn main() {"),
    ).toBeVisible();

    const rows = within(view).getAllByRole("row").slice(1);
    const cells = rows.map((r) =>
      within(r)
        .getAllByRole("cell")
        .map((c) => c.textContent),
    );
    expect(cells[1]).toEqual(["2", "", "two"]);
    expect(cells[2]).toEqual(["", "2", "TWO"]);
    expect(rows[1]).toHaveAttribute("data-kind", "removed");
    expect(within(rows[0]!).getByText("one")).toHaveClass("selectable");
    expect(
      within(view).getByLabelText("Sin salto de línea al final del archivo"),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: /main\.rs/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
  });

  it("closes with the back button or Escape", async () => {
    const user = userEvent.setup();
    ipc.commitFileDiff.mockResolvedValue({ status: "ok", data: textDiff() });
    renderWorkspace();

    const view = await openMainRs(user);
    await user.click(
      within(view).getByRole("button", { name: /Volver al historial/ }),
    );
    expect(screen.queryByRole("region", { name: /Diff de/ })).toBeNull();

    await openMainRs(user);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: /Diff de/ })).toBeNull();
  });

  it("asks before loading a very large diff", async () => {
    const user = userEvent.setup();
    ipc.commitFileDiff
      .mockResolvedValueOnce({
        status: "ok",
        data: textDiff({ tooLarge: true, lineCount: 12000, hunks: [] }),
      })
      .mockResolvedValueOnce({ status: "ok", data: textDiff() });
    renderWorkspace();

    const view = await openMainRs(user);
    expect(await within(view).findByText(/12000 líneas/)).toBeVisible();
    await user.click(
      within(view).getByRole("button", { name: "Mostrar de todos modos" }),
    );
    expect(ipc.commitFileDiff).toHaveBeenLastCalledWith(
      1,
      oid("d"),
      "src/main.rs",
      null,
      true,
    );
    expect(await within(view).findByText("TWO")).toBeVisible();
  });

  it("explains binary files, mode-only changes and errors", async () => {
    const user = userEvent.setup();
    ipc.commitFileDiff
      .mockResolvedValueOnce({
        status: "ok",
        data: textDiff({ binary: true, hunks: [] }),
      })
      .mockResolvedValueOnce({
        status: "ok",
        data: textDiff({
          notes: ["old mode 100644", "new mode 100755"],
          hunks: [],
        }),
      })
      .mockResolvedValueOnce({
        status: "error",
        error: commandError("unknown", "$ git diff-tree\nfatal: bad object"),
      });
    renderWorkspace();

    let view = await openMainRs(user);
    expect(await within(view).findByText(/Archivo binario/)).toBeVisible();
    await user.keyboard("{Escape}");

    view = await openMainRs(user);
    expect(await within(view).findByText("new mode 100755")).toBeVisible();
    expect(within(view).getByText(/No hay cambios de texto/)).toBeVisible();
    await user.keyboard("{Escape}");

    view = await openMainRs(user);
    const alert = await within(view).findByRole("alert");
    expect(within(alert).getByText(/fatal: bad object/)).toHaveClass(
      "selectable",
    );
  });

  it("closes when another commit is selected", async () => {
    const user = userEvent.setup();
    ipc.commitFileDiff.mockResolvedValue({ status: "ok", data: textDiff() });
    renderWorkspace();
    await openMainRs(user);
    useGraphStore.getState().select(1, oid("c"));
    expect(await screen.findAllByRole("option")).not.toHaveLength(0);
    expect(screen.queryByRole("region", { name: /Diff de/ })).toBeNull();
  });
});
