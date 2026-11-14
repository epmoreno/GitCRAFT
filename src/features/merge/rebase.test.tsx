import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { MovePreview, MoveResult } from "../../ipc";
import { cleanStatus, ipc, oid, sampleRefs } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { HistoryToast } from "../history/HistoryDialogs";
import { Sidebar } from "../sidebar/Sidebar";
import { useWorkdirStore } from "../workdir/store";
import { MergeDialog } from "./MergeDialog";
import { useMergeStore } from "./store";

const commit = (n: number, published = false) => ({
  commit: {
    oid: String(n).padStart(40, "b"),
    short: `r${n}`.padEnd(7, "0"),
    subject: `Commit ${n}`,
  },
  published,
});

/** Rebasing main (current) onto feature. */
function preview(overrides: Partial<MovePreview> = {}): MovePreview {
  return {
    kind: "rebase",
    source: {
      refname: "refs/heads/main",
      name: "main",
      remote: false,
      oid: oid("e"),
      short: "eeeeeee",
    },
    target: {
      refname: "refs/heads/feature",
      name: "feature",
      remote: false,
      oid: oid("d"),
      short: "ddddddd",
    },
    current: "main",
    moving: "main",
    refused: null,
    case: "rebase",
    commits: [commit(1), commit(2), commit(3)],
    commitsTotal: 3,
    skipped: 0,
    publishedIn: [],
    blocked: null,
    blocking: [],
    conflicts: [],
    stopsAt: null,
    exact: true,
    message: null,
    fingerprint: "fp-r",
    ...overrides,
  };
}

const done = (fastForward = false): MoveResult => ({
  outcome: { type: "done", head: oid("f"), fastForward, saved: null },
  undoId: 4,
});

async function open(p: MovePreview = preview(), target = "refs/heads/feature") {
  ipc.rebasePreview.mockResolvedValue({ status: "ok", data: p });
  render(
    <>
      <MergeDialog />
      <HistoryToast repoId={1} />
    </>,
  );
  await act(() =>
    useMergeStore.getState().requestRebase(1, "refs/heads/main", target),
  );
  return screen.getByRole("alertdialog");
}

const button = (dialog: HTMLElement, name: string) =>
  within(dialog).getByRole("button", { name });

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: sampleRefs(),
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
  ipc.undoAvailable.mockResolvedValue({ status: "ok", data: true });
  ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
});

describe("rebase", () => {
  it("is in a branch's menu, next to merge", async () => {
    const user = userEvent.setup();
    ipc.rebasePreview.mockResolvedValue({ status: "ok", data: preview() });
    render(
      <>
        <Sidebar repoId={1} />
        <MergeDialog />
      </>,
    );
    fireEvent.contextMenu(screen.getByRole("button", { name: /^feature/ }));
    await user.click(
      screen.getByRole("menuitem", {
        name: "Hacer rebase de «main» sobre «feature»…",
      }),
    );
    expect(ipc.rebasePreview).toHaveBeenCalledWith(
      1,
      "refs/heads/main",
      "refs/heads/feature",
    );
    expect(
      await screen.findByRole("alertdialog", {
        name: "¿Rebase de «main» sobre «feature»?",
      }),
    ).toBeInTheDocument();
  });

  it("lists the commits in the order they are replayed, then rebases", async () => {
    const user = userEvent.setup();
    const dialog = await open(preview({ skipped: 1 }));
    expect(dialog).toHaveTextContent(
      "Se mueve «main»: sus commits se rehacen encima de «feature», con hashes nuevos.",
    );
    expect(dialog).toHaveTextContent("Se rehacen 3 commits, en este orden:");
    const list = within(dialog).getAllByRole("list")[0]!;
    expect(list.tagName).toBe("OL");
    expect(
      within(list)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["r100000 Commit 1", "r200000 Commit 2", "r300000 Commit 3"]);
    expect(dialog).toHaveTextContent(
      "1 commit se salta porque «feature» ya tiene el mismo cambio.",
    );
    // Nothing published: no warning, no danger style.
    expect(dialog).not.toHaveTextContent("push forzado");
    expect(button(dialog, "Hacer rebase").className).not.toMatch(/danger/);

    ipc.rebaseBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(button(dialog, "Hacer rebase"));
    expect(ipc.rebaseBranch).toHaveBeenCalledWith(1, {
      source: "refs/heads/main",
      target: "refs/heads/feature",
      expected: "fp-r",
      saveChanges: false,
      allowConflicts: false,
    });
    expect(screen.getByRole("status")).toHaveTextContent(
      "Rebase de «main» sobre «feature» hecho: 3 commits rehechos.",
    );
  });

  it("warns about published commits, with the danger style", async () => {
    const user = userEvent.setup();
    const dialog = await open(
      preview({
        commits: [commit(1, true), commit(2, true), commit(3)],
        publishedIn: ["origin/main"],
      }),
    );
    expect(dialog).toHaveTextContent("r100000 Commit 1 publicado");
    expect(dialog).toHaveTextContent(
      "2 de estos commits ya están publicados (origin/main). Tras el rebase, «main» y su copia en el remoto habrán divergido: para subirlo hará falta un push forzado",
    );
    expect(dialog).toHaveTextContent(
      "GitCRAFT no lo hace solo: es una acción aparte, con su propia confirmación.",
    );
    // No box to tick (the user's choice): the danger style is enough.
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    const act_ = button(dialog, "Hacer rebase");
    expect(act_.className).toMatch(/danger/);
    expect(act_).toBeEnabled();
    ipc.rebaseBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(act_);
    expect(screen.getByRole("status")).toHaveTextContent(
      "3 commits rehechos. Para subirlo hará falta un push forzado.",
    );
  });

  it("says where it will stop with conflicts", async () => {
    const user = userEvent.setup();
    const dialog = await open(
      preview({
        conflicts: ["lista.txt"],
        stopsAt: { index: 2, commit: commit(2).commit },
      }),
    );
    expect(dialog).toHaveTextContent(
      "Se detendrá en el commit 2 de 3 («Commit 2») con conflictos en:",
    );
    expect(dialog).toHaveTextContent("lista.txt");
    expect(dialog).toHaveTextContent(
      "Lo que pase después de ese commit no se sabe hasta resolverlo.",
    );
    ipc.rebaseBranch.mockResolvedValue({
      status: "ok",
      data: {
        outcome: { type: "conflicted", files: ["lista.txt"], saved: null },
        undoId: null,
      },
    });
    await user.click(button(dialog, "Hacer rebase con conflictos"));
    expect(ipc.rebaseBranch).toHaveBeenCalledWith(
      1,
      expect.objectContaining({ allowConflicts: true }),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("changes in the way are saved first", async () => {
    const dialog = await open(
      preview({
        blocked: "rebaseNeedsClean",
        blocking: [{ path: "README.md", untracked: false }],
      }),
    );
    expect(dialog).toHaveTextContent(
      "git no hace un rebase con cambios sin confirmar en archivos con seguimiento:",
    );
    expect(dialog).toHaveTextContent("«GitCRAFT: antes del rebase de main»");
    expect(button(dialog, "Guardar mis cambios y hacer rebase")).toBeEnabled();
  });

  it("is not offered with merges in the range, and says why", async () => {
    const dialog = await open(preview({ refused: "mergesInRange" }));
    expect(dialog).toHaveTextContent(
      "Los commits que habría que rehacer incluyen merges. git los aplanaría y esos merges se perderían, así que no se ofrece el rebase. Puedes hacer merge en su lugar.",
    );
    expect(button(dialog, "Hacer rebase")).toBeDisabled();
  });

  it("a fast-forward only advances the branch", async () => {
    const dialog = await open(
      preview({ case: "fastForward", commits: [commit(1)], commitsTotal: 1 }),
    );
    expect(dialog).toHaveTextContent(
      "Ningún commit se rehace: «main» solo avanza hasta «feature» (fast-forward).",
    );
    expect(dialog).toHaveTextContent("«main» gana 1 commit:");
    expect(button(dialog, "Avanzar «main»")).toBeEnabled();
  });

  it("onto a remote branch: when it was last fetched", async () => {
    ipc.lastFetch.mockResolvedValue({
      status: "ok",
      data: { at: Math.floor(Date.now() / 1000) - 120, exact: false },
    });
    const dialog = await open(
      preview({
        target: {
          refname: "refs/remotes/origin/main",
          name: "origin/main",
          remote: true,
          oid: oid("d"),
          short: "ddddddd",
        },
      }),
      "refs/remotes/origin/main",
    );
    expect(ipc.lastFetch).toHaveBeenCalledWith(1, "origin");
    expect(dialog).toHaveTextContent(
      "Último fetch de «origin» que trajo cambios: hace 2 minutos.",
    );
    expect(button(dialog, "Fetch primero")).toBeEnabled();
  });

  it("«Deshacer» gives the branch its original commits back", async () => {
    const user = userEvent.setup();
    const dialog = await open();
    ipc.rebaseBranch.mockResolvedValue({ status: "ok", data: done() });
    await user.click(button(dialog, "Hacer rebase"));
    ipc.undoOperation.mockResolvedValue({ status: "ok", data: null });
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    expect(ipc.undoOperation).toHaveBeenCalledWith(1, 4);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Rebase deshecho: «main» vuelve a tener sus commits originales, exactamente como antes.",
    );
  });
});
