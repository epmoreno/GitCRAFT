import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { DiscardedFile, WorkingStatus } from "../../ipc";
import {
  cleanStatus,
  commandError,
  ipc,
  sampleLines,
  sampleRefs,
  streamEvents,
  textDiff,
} from "../../test/ipc";
import { App } from "../../app/App";
import { DetailPanel } from "../detail/DetailPanel";
import { GraphView } from "../graph/GraphView";
import { useGraphStore, WORKTREE } from "../graph/store";
import { DiscardDialog, UndoToast, UNDO_SECONDS } from "./Discard";
import { useWorkdirStore } from "./store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

const status = (): WorkingStatus => ({
  ...cleanStatus(),
  staged: [{ path: "staged.rs", oldPath: null, status: "modified" }],
  unstaged: [{ path: "src/main.rs", oldPath: null, status: "modified" }],
  untracked: [
    { path: "scratch.txt", oldPath: null, status: "untracked" },
    { path: "notes/", oldPath: null, status: "untracked" },
  ],
});

const loss = (overrides: Partial<DiscardedFile>): DiscardedFile => ({
  path: "src/main.rs",
  untracked: false,
  added: 3,
  removed: 1,
  lines: null,
  fileCount: null,
  undoUnsupported: false,
  ...overrides,
});

function renderWorkspace() {
  streamEvents([
    { type: "page", lines: sampleLines() },
    { type: "done", total: 6, suggestOptimize: false },
  ]);
  ipc.workingStatus.mockResolvedValue({ status: "ok", data: status() });
  ipc.worktreeFileDiff.mockResolvedValue({
    status: "ok",
    data: textDiff({ fingerprint: "fp" }),
  });
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
      <DiscardDialog />
      <UndoToast />
    </>,
  );
}

describe("discarding changes", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.discardChanges.mockResolvedValue({
      status: "ok",
      data: { undoId: 7, files: [loss({})], stashed: true },
    });
  });

  it("always asks first, saying exactly what is lost", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockResolvedValue({ status: "ok", data: [loss({})] });
    renderWorkspace();

    await user.click(
      screen.getByRole("button", {
        name: "Descartar los cambios de src/main.rs",
      }),
    );
    expect(ipc.discardPreview).toHaveBeenCalledWith(1, {
      kind: "files",
      paths: ["src/main.rs"],
    });
    expect(ipc.discardChanges).not.toHaveBeenCalled();

    const dialog = screen.getByRole("alertdialog", {
      name: "¿Descartar cambios?",
    });
    expect(dialog).toHaveTextContent(
      "Se perderán los cambios sin preparar de 1 archivo (+3 −1 líneas).",
    );
    expect(within(dialog).getByText("src/main.rs")).toBeVisible();
    expect(within(dialog).getByText("+3 −1")).toBeVisible();
    expect(dialog).toHaveTextContent(/copia en el stash/);
    expect(dialog).toHaveTextContent(/Los cambios preparados no se tocan/);
    // Focus starts on Cancel; Escape cancels without discarding.
    expect(
      within(dialog).getByRole("button", { name: "Cancelar" }),
    ).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(ipc.discardChanges).not.toHaveBeenCalled();
  });

  it("warns before confirming when this system cannot undo something", async () => {
    const user = userEvent.setup();
    const button = () =>
      screen.getByRole("button", {
        name: "Descartar los cambios de src/main.rs",
      });
    const warning =
      "En este sistema no se podrá deshacer con el botón; la copia queda en el stash.";
    ipc.discardPreview.mockResolvedValue({ status: "ok", data: [loss({})] });
    renderWorkspace();

    await user.click(button());
    expect(screen.getByRole("alertdialog")).not.toHaveTextContent(warning);
    await user.keyboard("{Escape}");

    ipc.discardPreview.mockResolvedValue({
      status: "ok",
      data: [loss({ undoUnsupported: true })],
    });
    await user.click(button());
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(warning);
    // It still saves the copy and can be confirmed.
    expect(dialog).toHaveTextContent(/copia en el stash/);
    await user.click(within(dialog).getByRole("button", { name: "Descartar" }));
    expect(ipc.discardChanges).toHaveBeenCalled();
  });

  it("explains that untracked files go to the Trash", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockResolvedValue({
      status: "ok",
      data: [
        loss({}),
        loss({
          path: "scratch.txt",
          untracked: true,
          added: null,
          removed: null,
          lines: 12,
        }),
        loss({
          path: "notes/",
          untracked: true,
          added: null,
          removed: null,
          fileCount: 3,
        }),
      ],
    });
    renderWorkspace();
    const unstaged = screen.getByRole("region", { name: "Sin preparar" });
    await user.click(
      within(unstaged).getByRole("button", { name: "Descartar todo" }),
    );
    expect(ipc.discardPreview).toHaveBeenCalledWith(1, {
      kind: "files",
      paths: ["src/main.rs", "scratch.txt", "notes/"],
    });
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      "2 elementos sin seguimiento irán a la Papelera.",
    );
    expect(within(dialog).getByText("12 líneas")).toBeVisible();
    expect(within(dialog).getByText("3 archivos")).toBeVisible();
    expect(dialog).toHaveTextContent(/recuperar desde la Papelera/);
  });

  it("discards on confirmation and offers Undo", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockResolvedValue({ status: "ok", data: [loss({})] });
    renderWorkspace();
    await user.click(
      screen.getByRole("button", {
        name: "Descartar los cambios de src/main.rs",
      }),
    );
    await user.click(screen.getByRole("button", { name: "Descartar" }));
    expect(ipc.discardChanges).toHaveBeenCalledWith(1, {
      kind: "files",
      paths: ["src/main.rs"],
    });

    const toast = await screen.findByRole("status");
    expect(toast).toHaveTextContent("Cambios descartados en 1 archivo.");
    expect(toast).toHaveTextContent("Copia guardada en el stash.");
    await user.click(within(toast).getByRole("button", { name: "Deshacer" }));
    expect(ipc.undoDiscard).toHaveBeenCalledWith(1, 7);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("explains why an undo was refused", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockResolvedValue({ status: "ok", data: [loss({})] });
    ipc.undoDiscard.mockResolvedValue({
      status: "error",
      error: commandError(
        "undoConflict",
        "cannot undo the discard of src/main.rs",
      ),
    });
    renderWorkspace();
    await user.click(
      screen.getByRole("button", {
        name: "Descartar los cambios de src/main.rs",
      }),
    );
    await user.click(screen.getByRole("button", { name: "Descartar" }));
    await user.click(await screen.findByRole("button", { name: "Deshacer" }));
    const toast = screen.getByRole("alert");
    expect(toast).toHaveTextContent(/no se ha tocado nada/);
    expect(toast).toHaveTextContent(/«Recuperar» en esa entrada/);
  });

  it("discards a hunk or selected lines from the diff, with the same confirmation", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockResolvedValue({
      status: "ok",
      data: [loss({ added: 1, removed: 1 })],
    });
    renderWorkspace();
    await user.click(
      screen.getByRole("button", { name: "Ver cambios de src/main.rs" }),
    );
    const view = await screen.findByRole("region", {
      name: "Diff de src/main.rs",
    });
    await user.click(
      await within(view).findByRole("button", { name: "Descartar hunk" }),
    );
    expect(ipc.discardPreview).toHaveBeenLastCalledWith(1, {
      kind: "selection",
      path: "src/main.rs",
      fingerprint: "fp",
      selection: [{ hunk: 0, lines: null }],
    });
    const dialog = screen.getByRole("alertdialog", {
      name: "¿Descartar líneas?",
    });
    expect(dialog).toHaveTextContent(
      "Se descartarán 2 líneas seleccionadas de src/main.rs (+1 −1).",
    );
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));

    await user.click(within(view).getAllByRole("checkbox")[1]!);
    await user.click(
      within(view).getByRole("button", { name: "Descartar 1 línea" }),
    );
    expect(ipc.discardPreview).toHaveBeenLastCalledWith(1, {
      kind: "selection",
      path: "src/main.rs",
      fingerprint: "fp",
      selection: [{ hunk: 0, lines: [2] }],
    });
  });

  it("offers no discard for staged or untracked diffs", async () => {
    const user = userEvent.setup();
    renderWorkspace();
    await user.click(
      screen.getByRole("button", { name: "Ver cambios de staged.rs" }),
    );
    let view = await screen.findByRole("region", { name: "Diff de staged.rs" });
    await within(view).findByText("TWO");
    expect(
      within(view).queryByRole("button", { name: /Descartar/ }),
    ).toBeNull();

    await user.keyboard("{Escape}");
    await user.click(
      screen.getByRole("button", { name: "Ver cambios de scratch.txt" }),
    );
    view = await screen.findByRole("region", { name: "Diff de scratch.txt" });
    await within(view).findByText("TWO");
    expect(
      within(view).queryByRole("button", { name: /Descartar/ }),
    ).toBeNull();
    // Staged rows in the panel have no discard button either.
    const staged = screen.getByRole("region", { name: "Preparados" });
    expect(
      within(staged).queryByRole("button", { name: /Descartar/ }),
    ).toBeNull();
  });
});

describe("undo toast", () => {
  it(`disappears after ${UNDO_SECONDS} seconds`, () => {
    vi.useFakeTimers();
    try {
      useWorkdirStore.setState({
        lastDiscard: {
          repoId: 1,
          undoId: 3,
          files: [loss({})],
          stashed: true,
          error: null,
        },
      });
      render(<UndoToast />);
      expect(screen.getByRole("status")).toBeVisible();
      act(() => vi.advanceTimersByTime((UNDO_SECONDS - 1) * 1000));
      expect(screen.getByRole("status")).toBeVisible();
      act(() => vi.advanceTimersByTime(1000));
      expect(screen.queryByRole("status")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("is part of the app shell", () => {
    useWorkdirStore.setState({
      lastDiscard: {
        repoId: 1,
        undoId: 3,
        files: [loss({})],
        stashed: false,
        error: null,
      },
    });
    render(<App />);
    const toast = screen.getByRole("status");
    expect(
      within(toast).getByRole("button", { name: "Deshacer" }),
    ).toBeVisible();
  });
});
