import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { MovePreview, MoveResult } from "../../ipc";
import { cleanStatus, ipc, oid, sampleRefs } from "../../test/ipc";
import { useGraphStore, type RepoGraph } from "../graph/store";
import { HistoryToast } from "../history/HistoryDialogs";
import { useHistoryStore } from "../history/store";
import { Sidebar } from "../sidebar/Sidebar";
import { useWorkdirStore } from "../workdir/store";
import { MergeDialog } from "./MergeDialog";
import { useMergeStore } from "./store";

const commit = (n: number) => ({
  commit: {
    oid: String(n).padStart(40, "a"),
    short: `c${n}`.padEnd(7, "0"),
    subject: `Commit ${n}`,
  },
  published: false,
});

function preview(overrides: Partial<MovePreview> = {}): MovePreview {
  return {
    kind: "merge",
    source: {
      refname: "refs/heads/feature",
      name: "feature",
      remote: false,
      oid: oid("d"),
      short: "ddddddd",
    },
    target: {
      refname: "refs/heads/main",
      name: "main",
      remote: false,
      oid: oid("e"),
      short: "eeeeeee",
    },
    current: "main",
    moving: "main",
    refused: null,
    case: "merge",
    commits: [commit(2), commit(1)],
    commitsTotal: 2,
    skipped: 0,
    publishedIn: [],
    blocked: null,
    blocking: [],
    conflicts: [],
    stopsAt: null,
    exact: true,
    message: "Merge branch 'feature'",
    fingerprint: "fp-1",
    ...overrides,
  };
}

const done = (
  outcome: Partial<Extract<MoveResult["outcome"], { type: "done" }>> = {},
): MoveResult => ({
  outcome: {
    type: "done",
    head: oid("f"),
    fastForward: false,
    saved: null,
    ...outcome,
  },
  undoId: 9,
});

const section = (name: RegExp) =>
  screen.getByRole("button", { name }).closest("section") as HTMLElement;

function withRefs(refs: RepoGraph["refs"]) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs,
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

async function open(p: MovePreview = preview(), source = "refs/heads/feature") {
  ipc.mergePreview.mockResolvedValue({ status: "ok", data: p });
  render(
    <>
      <MergeDialog />
      <HistoryToast repoId={1} />
    </>,
  );
  await act(() =>
    useMergeStore.getState().requestMerge(1, source, "refs/heads/main"),
  );
  return screen.getByRole("alertdialog");
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  withRefs(sampleRefs());
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
  ipc.undoAvailable.mockResolvedValue({ status: "ok", data: true });
  ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
});

describe("the merge menu", () => {
  it("is in a branch's menu in the sidebar, and opens the dialog", async () => {
    const user = userEvent.setup();
    ipc.mergePreview.mockResolvedValue({ status: "ok", data: preview() });
    render(
      <>
        <Sidebar repoId={1} />
        <MergeDialog />
      </>,
    );
    fireEvent.contextMenu(screen.getByRole("button", { name: /^feature/ }));
    const menu = screen.getByRole("menu");
    await user.click(
      within(menu).getByRole("menuitem", {
        name: "Hacer merge de «feature» en «main»…",
      }),
    );
    expect(ipc.mergePreview).toHaveBeenCalledWith(
      1,
      "refs/heads/feature",
      "refs/heads/main",
    );
    expect(
      await screen.findByRole("alertdialog", {
        name: "¿Merge de «feature» en «main»?",
      }),
    ).toBeInTheDocument();
  });

  it("is disabled, with the reason, on the current branch", () => {
    render(<Sidebar repoId={1} />);
    const local = section(/Ramas locales/);
    fireEvent.contextMenu(within(local).getByRole("button", { name: /^main/ }));
    const item = screen.getByRole("menuitem", {
      name: "Hacer merge de «main» en «main»…",
    });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "Es la rama actual.");
  });

  it("is in a remote branch's menu too", () => {
    render(<Sidebar repoId={1} />);
    const remotes = section(/Remotos/);
    fireEvent.contextMenu(
      within(remotes).getByRole("button", { name: "main" }),
    );
    expect(
      screen.getByRole("menuitem", {
        name: "Hacer merge de «origin/main» en «main»…",
      }),
    ).toBeEnabled();
  });

  it("opens from the keyboard on a sidebar branch (⇧F10)", async () => {
    const user = userEvent.setup();
    render(<Sidebar repoId={1} />);
    screen.getByRole("button", { name: /^feature/ }).focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(
      screen.getByRole("menuitem", {
        name: "Hacer merge de «feature» en «main»…",
      }),
    ).toBeInTheDocument();
  });
});

describe("the merge dialog", () => {
  it("shows what moves, the commits and git's message, then merges", async () => {
    const user = userEvent.setup();
    const dialog = await open();
    expect(dialog).toHaveTextContent(
      "Se mueve «main»: se crea en ella un commit de merge que une los cambios de «feature».",
    );
    expect(dialog).toHaveTextContent("Entran 2 commits en «main»:");
    expect(dialog).toHaveTextContent("c200000 Commit 2");
    const message = within(dialog).getByRole("textbox", {
      name: "Mensaje del commit de merge",
    });
    expect(message).toHaveValue("Merge branch 'feature'");
    await user.clear(message);
    await user.type(message, "Trae feature");
    ipc.mergeBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    );
    expect(ipc.mergeBranch).toHaveBeenCalledWith(1, {
      source: "refs/heads/feature",
      target: "refs/heads/main",
      expected: "fp-1",
      noFf: false,
      message: "Trae feature",
      saveChanges: false,
      allowConflicts: false,
    });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Merge de «feature» en «main» hecho.",
    );
  });

  it("cannot merge with an empty message", async () => {
    const user = userEvent.setup();
    const dialog = await open();
    await user.clear(within(dialog).getByRole("textbox"));
    expect(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    ).toBeDisabled();
  });

  it("lists at most 20 commits, then «y N más»", async () => {
    const commits = Array.from({ length: 50 }, (_, i) => commit(50 - i));
    const dialog = await open(preview({ commits, commitsTotal: 75 }));
    expect(dialog).toHaveTextContent("Entran 75 commits en «main»:");
    const list = within(dialog).getAllByRole("list")[0]!;
    expect(within(list).getAllByRole("listitem")).toHaveLength(21);
    expect(list).toHaveTextContent("y 55 más");
  });

  it("a fast-forward advances the branch, or makes a merge commit if asked", async () => {
    const user = userEvent.setup();
    const dialog = await open(preview({ case: "fastForward" }));
    expect(dialog).toHaveTextContent(
      "avanza hasta «feature» sin crear ningún commit",
    );
    expect(within(dialog).queryByRole("textbox")).toBeNull();
    const box = within(dialog).getByRole("checkbox", {
      name: "Crear siempre un commit de merge",
    });
    expect(box).not.toBeChecked();
    expect(
      within(dialog).getByRole("button", { name: "Avanzar «main»" }),
    ).toBeEnabled();
    await user.click(box);
    expect(within(dialog).getByRole("textbox")).toHaveValue(
      "Merge branch 'feature'",
    );
    ipc.mergeBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    );
    expect(ipc.mergeBranch).toHaveBeenCalledWith(
      1,
      expect.objectContaining({
        noFf: true,
        message: "Merge branch 'feature'",
      }),
    );
  });

  it("changes in the way and conflicts: saved first, then merged with conflicts", async () => {
    const user = userEvent.setup();
    const dialog = await open(
      preview({
        blocked: "stagedChanges",
        blocking: [{ path: "other.txt", untracked: false }],
        conflicts: ["f.txt"],
      }),
    );
    expect(dialog).toHaveTextContent(
      "git no hace un merge con cambios preparados, aunque sean de otros archivos:",
    );
    expect(dialog).toHaveTextContent("«GitCRAFT: antes del merge de feature»");
    expect(dialog).toHaveTextContent(
      "El merge producirá conflictos en 1 archivo:",
    );
    expect(dialog).toHaveTextContent(
      "Si lo abortas, tus cambios guardados seguirán en el stash",
    );
    ipc.mergeBranch.mockResolvedValue({
      status: "ok",
      data: {
        outcome: {
          type: "conflicted",
          files: ["f.txt"],
          saved: "5".repeat(40),
        },
        undoId: null,
      },
    });
    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y hacer merge con conflictos",
      }),
    );
    expect(ipc.mergeBranch).toHaveBeenCalledWith(
      1,
      expect.objectContaining({ saveChanges: true, allowConflicts: true }),
    );
    // Offered back once the merge is over (task 9's banner meanwhile).
    expect(useHistoryStore.getState().saved[1]).toEqual({
      oid: "5".repeat(40),
      pick: "merge",
      short: "feature",
    });
  });

  it("says why it cannot be done", async () => {
    const dialog = await open(
      preview({ refused: "notCurrent", moving: "feature", current: "main" }),
    );
    expect(dialog).toHaveTextContent(
      "Solo se hace merge en la rama actual, «main». Para hacerlo en «feature», cambia antes a ella.",
    );
    expect(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    ).toBeDisabled();
  });

  it("nothing to do when the branch is up to date", async () => {
    const dialog = await open(preview({ case: "upToDate", commits: [] }));
    expect(dialog).toHaveTextContent(
      "No hay nada que hacer: «main» ya tiene todos los commits de «feature».",
    );
    expect(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    ).toBeDisabled();
  });

  it("shows the new preview if the repository changed", async () => {
    const user = userEvent.setup();
    const dialog = await open();
    ipc.mergeBranch.mockResolvedValue({
      status: "ok",
      data: {
        outcome: {
          type: "notDone",
          preview: preview({ fingerprint: "fp-2", commitsTotal: 3 }),
          changed: true,
          saved: null,
        },
        undoId: null,
      },
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    );
    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "El repositorio ha cambiado desde la vista previa",
    );
    expect(dialog).toHaveTextContent("Entran 3 commits en «main»:");
  });

  it("from a remote branch: when it was last fetched, and «Fetch primero»", async () => {
    const user = userEvent.setup();
    const at = Math.floor(Date.now() / 1000) - 3 * 3600;
    ipc.lastFetch.mockResolvedValue({
      status: "ok",
      data: { at, exact: true },
    });
    const remote = preview({
      source: {
        refname: "refs/remotes/origin/main",
        name: "origin/main",
        remote: true,
        oid: oid("d"),
        short: "ddddddd",
      },
      message: "Merge remote-tracking branch 'origin/main'",
    });
    const dialog = await open(remote, "refs/remotes/origin/main");
    expect(ipc.lastFetch).toHaveBeenCalledWith(1, "origin");
    expect(dialog).toHaveTextContent("Último fetch de «origin»: hace 3 horas.");
    ipc.lastFetch.mockResolvedValue({
      status: "ok",
      data: { at: Math.floor(Date.now() / 1000), exact: true },
    });
    ipc.mergePreview.mockResolvedValue({
      status: "ok",
      data: { ...remote, commitsTotal: 4, fingerprint: "fp-3" },
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Fetch primero" }),
    );
    expect(ipc.fetchRemote).toHaveBeenCalledWith(
      1,
      "origin",
      false,
      expect.anything(),
    );
    expect(ipc.mergePreview).toHaveBeenCalledTimes(2);
    expect(dialog).toHaveTextContent("Entran 4 commits en «main»:");
    expect(dialog).toHaveTextContent("Último fetch de «origin»: ahora.");
  });

  it("«Deshacer» undoes the merge", async () => {
    const user = userEvent.setup();
    const dialog = await open();
    ipc.mergeBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer merge" }),
    );
    ipc.undoOperation.mockResolvedValue({ status: "ok", data: null });
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    expect(ipc.undoOperation).toHaveBeenCalledWith(1, 9);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Merge deshecho: «main» está exactamente como antes.",
    );
  });
});
