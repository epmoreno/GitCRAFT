import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { ResetMode, ResetPreview, WorkingStatus } from "../../ipc";
import { cleanStatus, commandError, ipc } from "../../test/ipc";
import { useWorkdirStore } from "../workdir/store";
import { CommitActionButtons } from "./CommitActionButtons";
import { HistoryDialogs, HistoryToast } from "./HistoryDialogs";
import { useHistoryStore } from "./store";

const OID = "c1c1c1c".padEnd(40, "0");
const STASH = "5".repeat(40);

const commit = (short: string, subject: string) => ({
  oid: short.padEnd(40, "0"),
  short,
  subject,
});

function preview(
  mode: ResetMode,
  overrides: Partial<ResetPreview> = {},
): ResetPreview {
  return {
    mode,
    branch: "main",
    target: commit("c1c1c1c", "c1"),
    refused: null,
    leaving: [
      { commit: commit("c3c3c3c", "c3"), onlyHere: true, published: false },
      { commit: commit("c2c2c2c", "c2"), onlyHere: false, published: true },
    ],
    leavingTotal: 2,
    publishedIn: ["origin/main"],
    lostChanges: [],
    unstagedByReset: [],
    overwrittenUntracked: [],
    savesStash: false,
    fingerprint: `fp-${mode}`,
    ...overrides,
  };
}

function serve(byMode: Partial<Record<ResetMode, Partial<ResetPreview>>> = {}) {
  ipc.resetPreview.mockImplementation((_id, _oid, mode) =>
    Promise.resolve({ status: "ok", data: preview(mode, byMode[mode]) }),
  );
}

async function open() {
  render(
    <>
      <HistoryDialogs />
      <HistoryToast repoId={1} />
    </>,
  );
  await act(() => useHistoryStore.getState().requestReset(1, OID));
  return screen.getByRole("alertdialog");
}

const status = (): WorkingStatus => cleanStatus();

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useWorkdirStore.setState({
    statuses: { 1: { status: status(), error: null } },
  });
  ipc.resetUndoAvailable.mockResolvedValue({ status: "ok", data: true });
});

describe("reset", () => {
  it("is in the panel, apart and with the danger style", async () => {
    render(<CommitActionButtons repoId={1} oid={OID} />);
    const button = await screen.findByRole("button", { name: "Restablecer…" });
    expect(button.className).toMatch(/danger/i);
  });

  it("starts in Mixed, never Hard, and lists what leaves the branch", async () => {
    serve();
    const dialog = await open();
    expect(ipc.resetPreview).toHaveBeenCalledWith(1, OID, "mixed");
    expect(within(dialog).getByRole("radio", { name: /Mixed/ })).toBeChecked();
    expect(
      within(dialog).getByRole("radio", { name: /Hard/ }),
    ).not.toBeChecked();
    expect(dialog).toHaveTextContent("Salen de «main» 2 commits:");
    expect(dialog).toHaveTextContent("c3c3c3c «c3» solo en esta rama");
    expect(dialog).toHaveTextContent("c2c2c2c «c2» publicado");
    // Point 3: where "only on this branch" commits can still be found.
    expect(dialog).toHaveTextContent(
      "Solo se recuperan con «Deshacer», justo después, o desde el historial de git (tarea 11).",
    );
    expect(dialog).toHaveTextContent(
      "1 commit ya está publicado (origin/main)",
    );
    // Mixed loses nothing: no box to tick.
    expect(
      within(dialog).getByRole("button", { name: "Restablecer «main»" }),
    ).toBeEnabled();
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
  });

  it("in Hard with something to lose, asks to tick a box first", async () => {
    const user = userEvent.setup();
    serve({
      hard: {
        lostChanges: ["f.txt"],
        overwrittenUntracked: ["h.txt"],
        savesStash: true,
      },
    });
    const dialog = await open();
    await user.click(within(dialog).getByRole("radio", { name: /Hard/ }));
    expect(
      await within(dialog).findByText(/Se descartan tus cambios/),
    ).toBeInTheDocument();
    expect(dialog).toHaveTextContent("«GitCRAFT: antes de reset a c1c1c1c»");
    expect(dialog).toHaveTextContent("h.txt (sin seguimiento)");
    const act_ = within(dialog).getByRole("button", {
      name: "Restablecer «main»",
    });
    expect(act_).toBeDisabled();
    await user.click(
      within(dialog).getByRole("checkbox", {
        name: "Entiendo que estos commits y cambios saldrán de «main».",
      }),
    );
    expect(act_).toBeEnabled();
    ipc.resetBranch.mockResolvedValue({
      status: "ok",
      data: { undoId: 7, stash: STASH, trashed: 1 },
    });
    await user.click(act_);
    expect(ipc.resetBranch).toHaveBeenCalledWith(1, OID, "hard", "fp-hard");
  });

  it("in Hard with nothing to lose, needs no box", async () => {
    const user = userEvent.setup();
    serve({
      hard: {
        leaving: [
          {
            commit: commit("c2c2c2c", "c2"),
            onlyHere: false,
            published: false,
          },
        ],
        leavingTotal: 1,
      },
    });
    const dialog = await open();
    await user.click(within(dialog).getByRole("radio", { name: /Hard/ }));
    await within(dialog).findByText(/Sale de «main» 1 commit/);
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    expect(
      within(dialog).getByRole("button", { name: "Restablecer «main»" }),
    ).toBeEnabled();
  });

  it("shows the new preview if the repository changed meanwhile", async () => {
    const user = userEvent.setup();
    serve();
    const dialog = await open();
    ipc.resetBranch.mockResolvedValue({
      status: "error",
      error: commandError("staleDiff"),
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Restablecer «main»" }),
    );
    expect(
      await screen.findByText(/ha cambiado desde la vista previa/),
    ).toBeInTheDocument();
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
  });

  it("offers Undo until another write, then the saved copy", async () => {
    const user = userEvent.setup();
    serve();
    const dialog = await open();
    ipc.resetBranch.mockResolvedValue({
      status: "ok",
      data: { undoId: 7, stash: STASH, trashed: 0 },
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Restablecer «main»" }),
    );
    expect(
      await screen.findByText("«main» restablecida a c1c1c1c."),
    ).toBeInTheDocument();
    const undo = screen.getByRole("button", { name: "Deshacer" });
    // No competing "Recover": Undo is the way back for now.
    expect(
      screen.queryByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeNull();

    // Another write: the status changes and Undo is no longer available.
    ipc.resetUndoAvailable.mockResolvedValue({ status: "ok", data: false });
    act(() =>
      useWorkdirStore.setState({
        statuses: { 1: { status: status(), error: null, version: 2 } },
      }),
    );
    expect(
      await screen.findByText(/Ya no se puede deshacer desde aquí/),
    ).toBeInTheDocument();
    expect(undo).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeInTheDocument();
  });

  it("undoes exactly, or says nothing was touched", async () => {
    const user = userEvent.setup();
    serve();
    const dialog = await open();
    ipc.resetBranch.mockResolvedValue({
      status: "ok",
      data: { undoId: 7, stash: null, trashed: 0 },
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Restablecer «main»" }),
    );
    ipc.undoReset.mockResolvedValueOnce({
      status: "error",
      error: commandError("undoConflict"),
    });
    await user.click(await screen.findByRole("button", { name: "Deshacer" }));
    expect(ipc.undoReset).toHaveBeenCalledWith(1, 7);
    expect(
      await screen.findByText(
        /Algo ha cambiado desde el reset, así que no se ha tocado nada/,
      ),
    ).toBeInTheDocument();

    act(() =>
      useHistoryStore.setState({
        notice: {
          kind: "reset",
          repoId: 1,
          undoId: 8,
          branch: "main",
          short: "c1c1c1c",
          undoGone: false,
        },
      }),
    );
    ipc.undoReset.mockResolvedValueOnce({ status: "ok", data: null });
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    expect(
      await screen.findByText(
        "Reset deshecho: «main» está exactamente como antes.",
      ),
    ).toBeInTheDocument();
  });
});
