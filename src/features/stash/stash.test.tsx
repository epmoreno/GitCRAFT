import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { StashEntry, WorkingStatus } from "../../ipc";
import { cleanStatus, ipc, oid, opened, sampleRefs } from "../../test/ipc";
import { Toolbar } from "../../app/Toolbar";
import { useGraphStore } from "../graph/store";
import { useRepoStore } from "../repo/store";
import { DetailPanel } from "../detail/DetailPanel";
import { Sidebar } from "../sidebar/Sidebar";
import { useWorkdirStore } from "../workdir/store";
import { StashDialogs, StashToast } from "./StashDialogs";
import { useStashStore } from "./store";

const now = Math.floor(Date.now() / 1000);

const entry = (overrides: Partial<StashEntry>): StashEntry => ({
  index: 0,
  oid: oid("1"),
  message: "mi trabajo",
  branch: "main",
  time: now - 2 * 3600,
  untracked: null,
  origin: "user",
  ...overrides,
});

const entries = (): StashEntry[] => [
  entry({
    index: 0,
    oid: oid("1"),
    message: "GitCRAFT: antes de cambiar a feature",
    origin: "beforeSwitch",
  }),
  entry({
    index: 1,
    oid: oid("2"),
    message: "GitCRAFT: descarte de c.txt",
    origin: "discard",
  }),
  entry({
    index: 2,
    oid: oid("3"),
    message: "mi trabajo",
    branch: "feature",
    untracked: oid("4"),
  }),
];

function renderSidebar() {
  ipc.listStashes.mockResolvedValue({ status: "ok", data: entries() });
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
  useStashStore.setState({
    stashes: { 1: { entries: entries(), error: null, readAt: Date.now() } },
  });
  render(
    <>
      <Sidebar repoId={1} />
      <StashDialogs />
      <StashToast />
    </>,
  );
}

const stashSection = () =>
  screen
    .getByRole("button", { name: /Stashes/ })
    .closest("section") as HTMLElement;

const item = (name: RegExp) =>
  within(stashSection()).getByRole("button", { name });

async function openMenu(
  user: ReturnType<typeof userEvent.setup>,
  name: RegExp,
) {
  await user.pointer({ keys: "[MouseRight]", target: item(name) });
}

describe("stash", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("lists every entry with branch and date, marking GitCRAFT's own", () => {
    renderSidebar();
    const section = stashSection();
    expect(within(section).getByText("3")).toBeVisible();

    const discard = item(/descarte de c\.txt/);
    expect(within(discard).getByText("descarte")).toBeVisible();
    expect(within(discard).getByText("main · hace 2 horas")).toBeVisible();
    const switching = item(/antes de cambiar a feature/);
    expect(within(switching).getByText("cambio de rama")).toBeVisible();

    const user = item(/^mi trabajo/);
    expect(within(user).getByText("feature · hace 2 horas")).toBeVisible();
    expect(within(user).queryByText("descarte")).not.toBeInTheDocument();
    expect(user.title).toMatch(/Rama: feature\nFecha: .+\nstash@\{2\}/);
  });

  it("applies an entry and keeps it", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await openMenu(user, /^mi trabajo/);
    await user.click(screen.getByRole("menuitem", { name: "Aplicar" }));
    expect(ipc.applyStash).toHaveBeenCalledWith(
      1,
      oid("3"),
      false,
      false,
      false,
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Entrada aplicada. Sigue en el stash.",
    );
  });

  it("never overwrites local changes: explains and offers to save them first", async () => {
    const user = userEvent.setup();
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: {
        type: "blocked",
        files: [
          { path: "f.txt", untracked: false },
          { path: "nuevo.txt", untracked: true },
        ],
      },
    });
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: { type: "applied", dropped: true },
    });
    renderSidebar();
    await openMenu(user, /^mi trabajo/);
    await user.click(screen.getByRole("menuitem", { name: /pop/ }));

    const dialog = await screen.findByRole("alertdialog", {
      name: "No se puede aplicar todavía",
    });
    expect(within(dialog).getByText("f.txt")).toBeVisible();
    expect(within(dialog).getByText("sin seguimiento")).toBeVisible();
    expect(within(dialog).getByText("No se ha tocado nada.")).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "Cancelar" }),
    ).toHaveFocus();

    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y aplicar",
      }),
    );
    expect(ipc.applyStash).toHaveBeenLastCalledWith(
      1,
      oid("3"),
      true,
      false,
      true,
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      /aplicada y borrada.*antes de aplicar/,
    );
  });

  it("reports conflicts before applying; pop keeps the entry if they are accepted", async () => {
    const user = userEvent.setup();
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: { type: "wouldConflict", files: ["f.txt"], exact: true },
    });
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: { type: "conflicted", files: ["f.txt"] },
    });
    renderSidebar();
    await openMenu(user, /^mi trabajo/);
    await user.click(screen.getByRole("menuitem", { name: /pop/ }));

    const dialog = await screen.findByRole("alertdialog", {
      name: "Aplicarla produciría conflictos",
    });
    expect(
      within(dialog).getByText(/1 archivo ha cambiado también/),
    ).toBeVisible();
    expect(within(dialog).getByText(/la entrada no se borrará/)).toBeVisible();
    await user.click(
      within(dialog).getByRole("button", { name: "Aplicar con conflictos" }),
    );
    expect(ipc.applyStash).toHaveBeenLastCalledWith(
      1,
      oid("3"),
      true,
      true,
      false,
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      /conflictos en 1 archivo.*sigue en el stash/,
    );
  });

  it("cancelling a conflict does nothing", async () => {
    const user = userEvent.setup();
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: { type: "wouldConflict", files: ["f.txt"], exact: false },
    });
    renderSidebar();
    await openMenu(user, /^mi trabajo/);
    await user.click(screen.getByRole("menuitem", { name: "Aplicar" }));
    expect(await screen.findByText(/podría chocar/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(ipc.applyStash).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("deleting always asks and shows how to recover the entry", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await openMenu(user, /descarte de c\.txt/);
    await user.click(screen.getByRole("menuitem", { name: "Borrar…" }));
    expect(ipc.dropStash).not.toHaveBeenCalled();

    const dialog = screen.getByRole("alertdialog", {
      name: "¿Borrar esta entrada del stash?",
    });
    expect(within(dialog).getByText(oid("2"))).toBeVisible();
    const command = `git stash store -m "GitCRAFT: descarte de c.txt" ${oid("2")}`;
    expect(within(dialog).getByText(command)).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "Cancelar" }),
    ).toHaveFocus();

    await user.click(
      within(dialog).getByRole("button", { name: "Borrar entrada" }),
    );
    expect(ipc.dropStash).toHaveBeenCalledWith(1, oid("2"));
    expect(
      within(await screen.findByRole("status")).getByText(command),
    ).toBeVisible();
  });

  it("creates an entry from the toolbar with a message and untracked files", async () => {
    const user = userEvent.setup();
    const status: WorkingStatus = {
      ...cleanStatus(),
      unstaged: [{ path: "a.txt", oldPath: null, status: "modified" }],
      untracked: [{ path: "b.txt", oldPath: null, status: "untracked" }],
    };
    useRepoStore.setState({
      tabs: [opened(1, "/src/sample")],
      activeId: 1,
    });
    useWorkdirStore.setState({ statuses: { 1: { status, error: null } } });
    render(
      <>
        <Toolbar />
        <StashDialogs />
        <StashToast />
      </>,
    );
    act(() =>
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
      }),
    );
    await user.click(screen.getByRole("button", { name: "Stash" }));

    const dialog = screen.getByRole("dialog", { name: "Guardar en el stash" });
    expect(within(dialog).getByText("Se guardará 1 archivo.")).toBeVisible();
    await user.type(within(dialog).getByRole("textbox"), "a medias");
    await user.click(within(dialog).getByRole("checkbox"));
    expect(within(dialog).getByText("Se guardarán 2 archivos.")).toBeVisible();
    await user.click(
      within(dialog).getByRole("button", { name: "Guardar en el stash" }),
    );
    expect(ipc.createStash).toHaveBeenCalledWith(1, "a medias", true);
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Cambios guardados en el stash.",
    );
  });

  it("shows an entry's files, untracked ones included, and their diffs", async () => {
    const user = userEvent.setup();
    ipc.commitChanges.mockImplementation(async (_id, commit) => ({
      status: "ok",
      data: {
        base: oid("0"),
        files:
          commit === oid("4")
            ? [
                {
                  path: "notas/nuevo.md",
                  oldPath: null,
                  similarity: null,
                  status: "added",
                  additions: 1,
                  deletions: 0,
                },
              ]
            : [
                {
                  path: "f.txt",
                  oldPath: null,
                  similarity: null,
                  status: "modified",
                  additions: 2,
                  deletions: 1,
                },
              ],
      },
    }));
    renderSidebar();
    render(<DetailPanel repoId={1} />);
    await user.click(item(/^mi trabajo/));
    expect(item(/^mi trabajo/)).toHaveAttribute("aria-pressed", "true");

    const panel = screen.getByRole("complementary", { name: "Detalle" });
    expect(
      within(panel).getByRole("heading", { name: "mi trabajo" }),
    ).toBeVisible();
    expect(within(panel).getByText("feature")).toBeVisible();
    expect(within(panel).getByText("stash@{2}")).toBeVisible();
    expect(await within(panel).findByText("1 archivo cambiado")).toBeVisible();
    expect(
      await within(panel).findByText("1 archivo sin seguimiento"),
    ).toBeVisible();
    expect(ipc.commitChanges).toHaveBeenCalledWith(1, oid("3"));
    expect(ipc.commitChanges).toHaveBeenCalledWith(1, oid("4"));

    await user.click(within(panel).getByRole("button", { name: /nuevo\.md/ }));
    expect(useGraphStore.getState().graphs[1]?.openFile).toEqual({
      kind: "commit",
      oid: oid("4"),
      path: "notas/nuevo.md",
      oldPath: null,
    });

    // Popping it from the panel stops showing it.
    ipc.applyStash.mockResolvedValueOnce({
      status: "ok",
      data: { type: "applied", dropped: true },
    });
    await user.click(within(panel).getByRole("button", { name: /pop/ }));
    expect(ipc.applyStash).toHaveBeenCalledWith(
      1,
      oid("3"),
      true,
      false,
      false,
    );
    await waitFor(() =>
      expect(useGraphStore.getState().graphs[1]?.selectedOid).toBeNull(),
    );
  });
});
