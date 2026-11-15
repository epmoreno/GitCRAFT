import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { StackTop, UndoPreview, UndoState } from "../../ipc";
import { setPlatformForTests } from "../../app/platform";
import { cleanStatus, ipc, oid } from "../../test/ipc";
import { HistoryToast } from "../history/HistoryDialogs";
import { useHistoryStore } from "../history/store";
import { useReflogStore } from "../reflog/store";
import { useWorkdirStore } from "../workdir/store";
import { UndoButtons } from "./UndoButtons";
import { UndoDialog } from "./UndoDialog";

const commitTop: StackTop = {
  id: 7,
  kind: { type: "commit", summary: "Añade la guía" },
  refused: null,
};

function state(undo: StackTop | null, redo: StackTop | null = null) {
  ipc.undoState.mockResolvedValue({
    status: "ok",
    data: { undo, redo } satisfies UndoState,
  });
}

const commitRef = (n: string, subject: string) => ({
  oid: oid(n),
  short: n.repeat(7),
  subject,
});

function preview(overrides: Partial<UndoPreview> = {}): UndoPreview {
  return {
    direction: "undo",
    kind: { type: "commit", summary: "Añade la guía" },
    refused: null,
    branches: [
      {
        branch: "main",
        from: oid("b"),
        to: oid("a"),
        leaving: [commitRef("b", "Añade la guía")],
        leavingTotal: 1,
        onlyInReflog: 1,
        arriving: [],
        arrivingTotal: 0,
      },
    ],
    tags: [],
    headFrom: { type: "branch", name: "refs/heads/main" },
    headTo: { type: "branch", name: "refs/heads/main" },
    exact: true,
    restoresStash: null,
    restoresTrashed: [],
    keptStash: null,
    keepsChanges: false,
    blocking: [],
    fingerprint: "fp-1",
    ...overrides,
  };
}

function renderAll() {
  render(
    <>
      <UndoButtons repoId={1} />
      <UndoDialog />
      <HistoryToast repoId={1} />
      <input aria-label="Resumen" />
    </>,
  );
}

const undoButton = () => screen.getByRole("button", { name: "Deshacer" });
const redoButton = () => screen.getByRole("button", { name: "Rehacer" });

async function openDialog(p: UndoPreview = preview()) {
  ipc.undoRedoPreview.mockResolvedValue({ status: "ok", data: p });
  renderAll();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Deshacer" }));
  return screen.findByRole("alertdialog");
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
  state(commitTop);
});

describe("the toolbar buttons", () => {
  it("say there is nothing to undo or redo", async () => {
    state(null);
    renderAll();
    await act(() => Promise.resolve());
    expect(undoButton()).toBeDisabled();
    expect(undoButton()).toHaveAccessibleDescription(
      "No hay nada que deshacer",
    );
    expect(redoButton()).toHaveAccessibleDescription("No hay nada que rehacer");
  });

  it("say what they undo, with the shortcut", async () => {
    state(commitTop, {
      id: 8,
      kind: { type: "createBranch", name: "nueva" },
      refused: null,
    });
    renderAll();
    await vi.waitFor(() => expect(undoButton()).toBeEnabled());
    expect(undoButton()).toHaveAccessibleDescription(
      "Deshacer el commit «Añade la guía» (⌘Z)",
    );
    expect(redoButton()).toHaveAccessibleDescription(
      "Rehacer la creación de la rama «nueva» (⇧⌘Z)",
    );
  });

  it("say why not when something changed or an operation waits", async () => {
    state(
      { ...commitTop, refused: { type: "changed" } },
      { ...commitTop, id: 9, refused: { type: "inProgress" } },
    );
    renderAll();
    await vi.waitFor(() =>
      expect(undoButton()).toHaveAccessibleDescription(
        /Ya no se puede deshacer el commit «Añade la guía»/,
      ),
    );
    expect(undoButton()).toBeDisabled();
    expect(redoButton()).toHaveAccessibleDescription(
      "Hay una operación a medias: termínala o abórtala antes",
    );
  });
});

describe("the shortcuts", () => {
  it("⌘Z opens the dialog outside text fields, not inside them", async () => {
    ipc.undoRedoPreview.mockResolvedValue({ status: "ok", data: preview() });
    renderAll();
    await vi.waitFor(() => expect(undoButton()).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Resumen" }), {
      key: "z",
      metaKey: true,
    });
    await act(() => Promise.resolve());
    expect(ipc.undoRedoPreview).not.toHaveBeenCalled();

    fireEvent.keyDown(document.body, { key: "z", metaKey: true });
    expect(await screen.findByRole("alertdialog")).toBeVisible();
    expect(ipc.undoRedoPreview).toHaveBeenCalledWith(1, 7, "undo");
  });

  it("⇧⌘Z redoes on macOS, Ctrl+Y and Ctrl+Shift+Z elsewhere", async () => {
    state(null, { ...commitTop, id: 3 });
    ipc.undoRedoPreview.mockResolvedValue({
      status: "ok",
      data: preview({ direction: "redo" }),
    });
    renderAll();
    await vi.waitFor(() => expect(redoButton()).toBeEnabled());
    fireEvent.keyDown(document.body, {
      key: "Z",
      metaKey: true,
      shiftKey: true,
    });
    expect(await screen.findByRole("alertdialog")).toBeVisible();
    expect(ipc.undoRedoPreview).toHaveBeenLastCalledWith(1, 3, "redo");
  });

  it("Ctrl+Y redoes on Windows and Linux", async () => {
    setPlatformForTests("windows");
    state(null, { ...commitTop, id: 3 });
    ipc.undoRedoPreview.mockResolvedValue({
      status: "ok",
      data: preview({ direction: "redo" }),
    });
    renderAll();
    await vi.waitFor(() => expect(redoButton()).toBeEnabled());
    expect(redoButton()).toHaveAccessibleDescription(/\(Ctrl\+Mayús\+Z\)/);
    fireEvent.keyDown(document.body, { key: "y", ctrlKey: true });
    expect(await screen.findByRole("alertdialog")).toBeVisible();
  });
});

describe("the dialog", () => {
  it("undoing a commit: what leaves, its changes stay staged", async () => {
    const dialog = await openDialog();
    expect(
      within(dialog).getByRole("heading", {
        name: "¿Deshacer el commit «Añade la guía»?",
      }),
    ).toBeVisible();
    expect(dialog).toHaveTextContent("«main» pasa a aaaaaaa.");
    expect(dialog).toHaveTextContent("Sale de «main» 1 commit:");
    expect(dialog).toHaveTextContent("bbbbbbb «Añade la guía»");
    expect(dialog).toHaveTextContent(
      "1 de ellos no está en ninguna otra rama, tag ni remoto",
    );
    expect(dialog).toHaveTextContent("Sus cambios quedan preparados");
    expect(dialog).toHaveTextContent(
      "Todo vuelve exactamente como estaba antes",
    );

    ipc.undoRedo.mockResolvedValue({
      status: "ok",
      data: { type: "done", saved: null, exact: true, unstaged: false },
    });
    state(null, commitTop);
    const user = userEvent.setup();
    await user.click(within(dialog).getByRole("button", { name: "Deshacer" }));
    expect(ipc.undoRedo).toHaveBeenCalledWith(1, 7, {
      direction: "undo",
      expected: "fp-1",
      saveChanges: false,
    });
    const toast = await screen.findByRole("status");
    expect(toast).toHaveTextContent("Deshecho: el commit «Añade la guía».");

    // The way forward again: the redo dialog.
    ipc.undoRedoPreview.mockResolvedValue({
      status: "ok",
      data: preview({ direction: "redo" }),
    });
    await user.click(within(toast).getByRole("button", { name: "Rehacer" }));
    expect(ipc.undoRedoPreview).toHaveBeenLastCalledWith(1, 7, "redo");
    expect(
      await screen.findByRole("heading", {
        name: "¿Rehacer el commit «Añade la guía»?",
      }),
    ).toBeVisible();
  });

  it("changes in the way: saved in the stash only through their button", async () => {
    const dialog = await openDialog(
      preview({
        exact: false,
        blocking: [{ path: "notas.txt", untracked: true }],
      }),
    );
    expect(dialog).toHaveTextContent("Tus cambios sin confirmar se conservan.");
    expect(dialog).toHaveTextContent("notas.txt");
    ipc.undoRedo.mockResolvedValue({
      status: "ok",
      data: { type: "done", saved: oid("5"), exact: false, unstaged: false },
    });
    const user = userEvent.setup();
    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y deshacer",
      }),
    );
    expect(ipc.undoRedo).toHaveBeenCalledWith(1, 7, {
      direction: "undo",
      expected: "fp-1",
      saveChanges: true,
    });
    expect(useHistoryStore.getState().saved[1]?.oid).toBe(oid("5"));
  });

  it("a redo that keeps the changes says so", async () => {
    state(null, {
      ...commitTop,
      kind: { type: "reset", mode: "hard", target: oid("c") },
    });
    ipc.undoRedoPreview.mockResolvedValue({
      status: "ok",
      data: preview({
        direction: "redo",
        kind: { type: "reset", mode: "hard", target: oid("c") },
        exact: false,
        keepsChanges: true,
      }),
    });
    renderAll();
    await vi.waitFor(() => expect(redoButton()).toBeEnabled());
    const user = userEvent.setup();
    await user.click(redoButton());
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("¿Rehacer el reset a ccccccc?");
    expect(dialog).toHaveTextContent(
      "Tus cambios sin confirmar se quedan como están: no se descartan otra vez.",
    );
  });

  it("a published commit: refused, with Revert instead", async () => {
    const published = commitRef("b", "Añade la guía");
    const dialog = await openDialog(
      preview({
        refused: {
          type: "published",
          branch: "main",
          commits: [published],
          remotes: ["origin/main"],
        },
      }),
    );
    expect(dialog).toHaveTextContent(
      "No se puede deshacer: el commit bbbbbbb «Añade la guía» ya está en origin/main.",
    );
    expect(dialog).toHaveTextContent("push forzado");
    expect(
      within(dialog).queryByRole("button", { name: "Deshacer" }),
    ).toBeNull();
    ipc.revertPreview.mockResolvedValue({
      status: "error",
      error: { kind: "cancelled", message: "", details: null },
    } as never);
    const user = userEvent.setup();
    await user.click(
      within(dialog).getByRole("button", { name: "Revertir «Añade la guía»…" }),
    );
    expect(ipc.revertPreview).toHaveBeenCalledWith(1, published.oid);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("several published commits: explained, nothing to press", async () => {
    const dialog = await openDialog(
      preview({
        refused: {
          type: "published",
          branch: "main",
          commits: [commitRef("b", "B"), commitRef("c", "C")],
          remotes: ["origin/main"],
        },
      }),
    );
    expect(dialog).toHaveTextContent(
      "2 commits que saldrían de «main» ya están en origin/main",
    );
    expect(
      within(dialog).getByRole("button", { name: "Deshacer" }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: "Cerrar" }),
    ).toBeEnabled();
  });

  it("something changed: the branch history is the way back", async () => {
    const dialog = await openDialog(preview({ refused: { type: "changed" } }));
    expect(dialog).toHaveTextContent("usa el historial de la rama");
    expect(
      within(dialog).queryByRole("button", { name: "Deshacer" }),
    ).toBeNull();
    ipc.branchReflog.mockResolvedValue({
      status: "ok",
      data: {
        refname: "refs/heads/main",
        entries: [],
        more: false,
        expiry: {
          reachable: { type: "days", days: 90 },
          unreachable: { type: "days", days: 30 },
        },
      },
    });
    const user = userEvent.setup();
    await user.click(
      within(dialog).getByRole("button", {
        name: "Ver el historial de «main»",
      }),
    );
    expect(ipc.branchReflog).toHaveBeenCalledWith(1, "refs/heads/main");
    expect(useReflogStore.getState().dialog?.refname).toBe("refs/heads/main");
  });

  it("an operation waiting: nothing to press", async () => {
    const dialog = await openDialog(
      preview({ refused: { type: "inProgress" } }),
    );
    expect(dialog).toHaveTextContent("a medias");
    expect(
      within(dialog).getByRole("button", { name: "Deshacer" }),
    ).toBeDisabled();
  });

  it("shows the new preview when the repository changed meanwhile", async () => {
    const dialog = await openDialog();
    ipc.undoRedo.mockResolvedValue({
      status: "ok",
      data: {
        type: "notDone",
        preview: preview({ fingerprint: "fp-2", exact: false }),
      },
    });
    const user = userEvent.setup();
    await user.click(within(dialog).getByRole("button", { name: "Deshacer" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "El repositorio ha cambiado desde la vista previa",
    );
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "Tus cambios sin confirmar se conservan.",
    );
  });

  it("branches, tags and checkouts in their own words", async () => {
    const dialog = await openDialog(
      preview({
        kind: { type: "deleteBranch", name: "feature" },
        exact: false,
        branches: [
          {
            branch: "feature",
            from: null,
            to: oid("f"),
            leaving: [],
            leavingTotal: 0,
            onlyInReflog: 0,
            arriving: [commitRef("e", "F2"), commitRef("d", "F1")],
            arrivingTotal: 2,
          },
        ],
        tags: [{ refname: "refs/tags/v1", before: oid("1"), after: null }],
        headTo: { type: "branch", name: "refs/heads/feature" },
      }),
    );
    expect(dialog).toHaveTextContent(
      "¿Deshacer el borrado de la rama «feature»?",
    );
    expect(dialog).toHaveTextContent("Se crea la rama «feature» en fffffff.");
    expect(dialog).toHaveTextContent("Entran en «feature» 2 commits:");
    expect(dialog).toHaveTextContent("Se borra el tag «v1».");
    expect(dialog).toHaveTextContent(
      "Pasas a la rama «feature»; tus cambios sin confirmar vienen contigo.",
    );
  });
});

describe("the notices of task 8 and 10", () => {
  it("offer Redo after undoing a merge or a reset", async () => {
    useHistoryStore.setState({
      notice: { kind: "mergeUndone", op: "merge", repoId: 1, branch: "main" },
    });
    state(null, {
      ...commitTop,
      kind: { type: "merge", source: "feature", fastForward: false },
    });
    ipc.undoRedoPreview.mockResolvedValue({
      status: "ok",
      data: preview({
        direction: "redo",
        kind: { type: "merge", source: "feature", fastForward: false },
      }),
    });
    renderAll();
    const toast = screen.getByRole("status");
    const user = userEvent.setup();
    await user.click(within(toast).getByRole("button", { name: "Rehacer" }));
    expect(
      await screen.findByRole("heading", {
        name: "¿Rehacer el merge de «feature»?",
      }),
    ).toBeVisible();

    act(() => {
      useHistoryStore.setState({
        notice: { kind: "resetUndone", repoId: 1, branch: "main" },
      });
    });
    expect(
      within(screen.getByRole("status")).getByRole("button", {
        name: "Rehacer",
      }),
    ).toBeVisible();
  });
});
