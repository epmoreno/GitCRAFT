import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { WorkingStatus } from "../../ipc";
import {
  cleanStatus,
  commandError,
  ipc,
  sampleLines,
  sampleRefs,
  streamEvents,
  textDiff,
} from "../../test/ipc";
import { DetailPanel } from "../detail/DetailPanel";
import { GraphView } from "../graph/GraphView";
import { useGraphStore, WORKTREE } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

const status = (): WorkingStatus => ({
  ...cleanStatus(),
  staged: [{ path: "src/staged.rs", oldPath: null, status: "modified" }],
  unstaged: [{ path: "src/main.rs", oldPath: null, status: "modified" }],
  untracked: [
    { path: "new.txt", oldPath: null, status: "untracked" },
    { path: "notes/", oldPath: null, status: "untracked" },
  ],
});

/** Two hunks: two → TWO, and ten → TEN. */
const twoHunks = () =>
  textDiff({
    fingerprint: "abc",
    hunks: [
      ...textDiff().hunks,
      {
        oldStart: 10,
        oldLines: 1,
        newStart: 10,
        newLines: 1,
        section: "",
        lines: [
          {
            kind: "removed",
            content: "ten",
            oldLine: 10,
            newLine: null,
            noNewline: false,
          },
          {
            kind: "added",
            content: "TEN",
            oldLine: null,
            newLine: 10,
            noNewline: false,
          },
        ],
      },
    ],
  });

function renderWorkspace() {
  streamEvents([
    { type: "page", lines: sampleLines() },
    { type: "done", total: 6, suggestOptimize: false },
  ]);
  ipc.workingStatus.mockResolvedValue({ status: "ok", data: status() });
  ipc.worktreeFileDiff.mockResolvedValue({ status: "ok", data: twoHunks() });
  useWorkdirStore.setState({
    statuses: { 1: { status: status(), error: null } },
  });
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: sampleLines(),
        maxColumn: 1,
        refs: sampleRefs(),
        error: null,
        selectedOid: WORKTREE,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
  render(
    <>
      <GraphView repoId={1} info={info} />
      <DetailPanel repoId={1} />
    </>,
  );
}

async function open(user: ReturnType<typeof userEvent.setup>, path: string) {
  await user.click(
    screen.getByRole("button", { name: `Ver cambios de ${path}` }),
  );
  return screen.findByRole("region", { name: `Diff de ${path}` });
}

describe("partial staging", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("opens the diff of an uncommitted file in the right area", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    const view = await open(user, "src/main.rs");
    expect(ipc.worktreeFileDiff).toHaveBeenCalledWith(
      1,
      "src/main.rs",
      null,
      "unstaged",
      false,
    );
    expect(within(view).getByText("Sin preparar")).toBeVisible();
    expect(await within(view).findByText("TWO")).toBeVisible();

    await user.keyboard("{Escape}");
    await open(user, "new.txt");
    expect(ipc.worktreeFileDiff).toHaveBeenLastCalledWith(
      1,
      "new.txt",
      null,
      "untracked",
      false,
    );

    await user.keyboard("{Escape}");
    await open(user, "src/staged.rs");
    expect(ipc.worktreeFileDiff).toHaveBeenLastCalledWith(
      1,
      "src/staged.rs",
      null,
      "staged",
      false,
    );
    // Untracked directories have no single-file diff.
    expect(
      screen.queryByRole("button", { name: "Ver cambios de notes/" }),
    ).toBeNull();
  });

  it("stages a whole hunk with the diff's fingerprint", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    const view = await open(user, "src/main.rs");
    const buttons = await within(view).findAllByRole("button", {
      name: "Preparar hunk",
    });
    await user.click(buttons[1]!);
    expect(ipc.applySelection).toHaveBeenCalledWith(
      1,
      "src/main.rs",
      null,
      "unstaged",
      "abc",
      [{ hunk: 1, lines: null }],
      "stage",
    );
  });

  it("stages selected lines, with shift-click ranges", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    const view = await open(user, "src/main.rs");
    expect(await within(view).findByText(/Marca líneas/)).toBeVisible();

    // Hunk 0: lines 1 (−two) and 2 (+TWO); hunk 1: line 1 (+TEN).
    const boxes = within(view).getAllByRole("checkbox");
    expect(boxes).toHaveLength(4); // only changed lines can be picked
    await user.click(boxes[0]!); // −two
    await user.click(boxes[3]!); // +TEN
    const stageLines = within(view).getByRole("button", {
      name: "Preparar 2 líneas",
    });
    await user.click(stageLines);
    expect(ipc.applySelection).toHaveBeenCalledWith(
      1,
      "src/main.rs",
      null,
      "unstaged",
      "abc",
      [
        { hunk: 0, lines: [1] },
        { hunk: 1, lines: [1] },
      ],
      "stage",
    );
  });

  it("selects a range within a hunk with shift-click on rows", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    const view = await open(user, "src/main.rs");
    await within(view).findByText("TWO");
    await user.click(within(view).getByText("two"));
    await user.keyboard("{Shift>}");
    await user.click(within(view).getByText("TWO"));
    await user.keyboard("{/Shift}");
    expect(
      within(view).getByRole("button", { name: "Preparar 2 líneas" }),
    ).toBeVisible();
    await user.click(
      within(view).getByRole("button", { name: "Deseleccionar" }),
    );
    expect(
      within(view).queryByRole("button", { name: /Preparar 2/ }),
    ).toBeNull();
  });

  it("unstages from the staged area", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    const view = await open(user, "src/staged.rs");
    await user.click(
      (await within(view).findAllByRole("button", { name: "Quitar hunk" }))[0]!,
    );
    expect(ipc.applySelection).toHaveBeenCalledWith(
      1,
      "src/staged.rs",
      null,
      "staged",
      "abc",
      [{ hunk: 0, lines: null }],
      "unstage",
    );
  });

  it("re-reads the diff after the status changes and reports stale diffs", async () => {
    const user = userEvent.setup();
    ipc.applySelection.mockResolvedValue({
      status: "error",
      error: commandError("staleDiff"),
    });
    renderWorkspace();
    const view = await open(user, "src/main.rs");
    const calls = ipc.worktreeFileDiff.mock.calls.length;
    await user.click(
      (
        await within(view).findAllByRole("button", { name: "Preparar hunk" })
      )[0]!,
    );
    // The status is re-read after every action; the diff follows it.
    expect(ipc.worktreeFileDiff.mock.calls.length).toBeGreaterThan(calls);
    const panel = screen.getByRole("complementary", { name: "Detalle" });
    expect(
      await within(panel).findByText(/El archivo cambió desde que se mostró/),
    ).toBeVisible();
  });

  it("offers no staging in commit diffs", async () => {
    ipc.commitFileDiff.mockResolvedValue({ status: "ok", data: twoHunks() });
    renderWorkspace();
    useGraphStore.getState().openFileDiff(1, {
      kind: "commit",
      oid: "d".repeat(40),
      path: "src/main.rs",
      oldPath: null,
    });
    const view = await screen.findByRole("region", {
      name: "Diff de src/main.rs",
    });
    await within(view).findByText("TWO");
    expect(within(view).queryByRole("checkbox")).toBeNull();
    expect(within(view).queryByRole("button", { name: /hunk/ })).toBeNull();
  });
});
