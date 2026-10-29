import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { BranchDeletion, RepoRefs } from "../../ipc";
import { commandError, ipc, oid, sampleRefs } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { Sidebar } from "../sidebar/Sidebar";
import { BranchDialogs, BranchToast } from "./BranchDialogs";

function renderSidebar(refs: RepoRefs = sampleRefs()) {
  ipc.getRefs.mockResolvedValue({ status: "ok", data: refs });
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
  render(
    <>
      <Sidebar repoId={1} />
      <BranchDialogs />
      <BranchToast />
    </>,
  );
}

/** A local branch in the sidebar. */
const branch = (name: string) =>
  within(
    screen
      .getByRole("button", { name: /Ramas locales/ })
      .closest("section") as HTMLElement,
  ).getByRole("button", { name: new RegExp(`^${name}`) });

const deletion = (overrides: Partial<BranchDeletion> = {}): BranchDeletion => ({
  name: "feature",
  tip: oid("d"),
  upstream: null,
  orphaned: 2,
  commits: [
    { oid: oid("d"), summary: "D", author: "Ana", time: 0 },
    { oid: oid("c"), summary: "C", author: "Ana", time: 0 },
  ],
  ...overrides,
});

describe("branches", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("switches branch with a double click", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await user.dblClick(branch("feature"));
    expect(ipc.checkoutBranch).toHaveBeenCalledWith(1, "feature");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("never forces a switch: explains the blocking files and offers to stash", async () => {
    const user = userEvent.setup();
    ipc.checkoutBranch.mockResolvedValue({
      status: "ok",
      data: {
        type: "blocked",
        files: [
          { path: "src/app.rs", untracked: false },
          { path: "notes.txt", untracked: true },
        ],
        changed: 3,
      },
    });
    renderSidebar();
    await user.dblClick(branch("feature"));

    const dialog = await screen.findByRole("alertdialog", {
      name: "No se puede cambiar a «feature» todavía",
    });
    expect(within(dialog).getByText("src/app.rs")).toBeVisible();
    expect(within(dialog).getByText("sin seguimiento")).toBeVisible();
    expect(within(dialog).getByText(/No se ha tocado nada/)).toBeVisible();
    expect(within(dialog).getByText(/\(3 archivos\)/)).toBeVisible();
    // Focus starts on Cancel.
    expect(
      within(dialog).getByRole("button", { name: "Cancelar" }),
    ).toHaveFocus();

    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar en el stash y cambiar",
      }),
    );
    expect(ipc.stashAndCheckout).toHaveBeenCalledWith(1, "feature", true);
    expect(
      await screen.findByText(/GitCRAFT: antes de cambiar a feature/),
    ).toBeVisible();
  });

  it("cancelling a blocked switch does nothing", async () => {
    const user = userEvent.setup();
    ipc.checkoutBranch.mockResolvedValue({
      status: "ok",
      data: {
        type: "blocked",
        files: [{ path: "a.txt", untracked: false }],
        changed: 1,
      },
    });
    renderSidebar();
    await user.dblClick(branch("feature"));
    await user.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(ipc.stashAndCheckout).not.toHaveBeenCalled();
  });

  it("deleting a branch lists the commits left without one", async () => {
    const user = userEvent.setup();
    ipc.branchDeletionPreview.mockResolvedValue({
      status: "ok",
      data: deletion({ upstream: "origin/feature" }),
    });
    renderSidebar();
    await user.pointer({ keys: "[MouseRight]", target: branch("feature") });
    await user.click(screen.getByRole("menuitem", { name: "Borrar…" }));
    expect(ipc.branchDeletionPreview).toHaveBeenCalledWith(1, "feature");

    const dialog = await screen.findByRole("alertdialog", {
      name: "¿Borrar la rama «feature»?",
    });
    expect(
      within(dialog).getByText(/tiene 2 commits que no están en ninguna otra/),
    ).toBeVisible();
    expect(within(dialog).getByText("ddddddd")).toBeVisible();
    expect(within(dialog).getByText("C")).toBeVisible();
    expect(
      within(dialog).getByText(/La rama remota «origin\/feature» no se borra/),
    ).toBeVisible();
    expect(
      within(dialog).getByText(`git branch feature ${oid("d")}`),
    ).toBeVisible();

    await user.click(
      within(dialog).getByRole("button", { name: "Borrar rama" }),
    );
    expect(ipc.deleteBranch).toHaveBeenCalledWith(1, "feature", oid("d"));
    const toast = await screen.findByRole("status");
    expect(
      within(toast).getByText(`git branch feature ${oid("d")}`),
    ).toBeVisible();
  });

  it("says when deleting loses no commit", async () => {
    const user = userEvent.setup();
    ipc.branchDeletionPreview.mockResolvedValue({
      status: "ok",
      data: deletion({ orphaned: 0, commits: [] }),
    });
    renderSidebar();
    await user.pointer({ keys: "[MouseRight]", target: branch("feature") });
    await user.click(screen.getByRole("menuitem", { name: "Borrar…" }));
    expect(await screen.findByText(/No se pierde ningún commit/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(ipc.deleteBranch).not.toHaveBeenCalled();
  });

  it("the current branch cannot be deleted or switched to", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await user.pointer({ keys: "[MouseRight]", target: branch("main") });
    expect(screen.getByRole("menuitem", { name: "Borrar…" })).toBeDisabled();
    expect(
      screen.getByRole("menuitem", { name: "Cambiar a «main»" }),
    ).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("reports a branch that moved since the confirmation", async () => {
    const user = userEvent.setup();
    ipc.branchDeletionPreview.mockResolvedValue({
      status: "ok",
      data: deletion(),
    });
    ipc.deleteBranch.mockResolvedValue({
      status: "error",
      error: commandError("branchChanged"),
    });
    renderSidebar();
    await user.pointer({ keys: "[MouseRight]", target: branch("feature") });
    await user.click(screen.getByRole("menuitem", { name: "Borrar…" }));
    await user.click(
      await screen.findByRole("button", { name: "Borrar rama" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /La rama ha cambiado desde que se mostró/,
    );
  });

  it("creates a branch from HEAD, checking the name as it is typed", async () => {
    const user = userEvent.setup();
    ipc.checkBranchName.mockImplementation(async (_id, name) => ({
      status: "ok",
      data: name.includes("..")
        ? "invalid"
        : name === "feature"
          ? "exists"
          : "valid",
    }));
    ipc.createBranch.mockResolvedValue({
      status: "ok",
      data: { type: "switched" },
    });
    renderSidebar();
    await user.click(screen.getByRole("button", { name: "Nueva rama" }));

    const dialog = screen.getByRole("dialog", { name: "Nueva rama" });
    expect(within(dialog).getByText("Empieza en «main».")).toBeVisible();
    const input = within(dialog).getByRole("textbox", { name: "Nombre" });
    const create = within(dialog).getByRole("button", { name: "Crear rama" });

    expect(create).toBeDisabled(); // empty
    await user.type(input, "a..b");
    expect(await within(dialog).findByText(/no es válido/)).toBeVisible();
    expect(create).toBeDisabled();

    await user.clear(input);
    await user.type(input, "feature");
    expect(
      await within(dialog).findByText("Ya existe una rama llamada «feature»."),
    ).toBeVisible();
    expect(create).toBeDisabled();

    await user.clear(input);
    await user.type(input, "nueva/idea");
    await waitFor(() => expect(create).toBeEnabled());
    await user.click(create);
    expect(ipc.createBranch).toHaveBeenCalledWith(
      1,
      "nueva/idea",
      oid("e"),
      false,
      true,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renames a branch", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await user.pointer({ keys: "[MouseRight]", target: branch("feature") });
    await user.click(screen.getByRole("menuitem", { name: "Renombrar…" }));
    const dialog = screen.getByRole("dialog", { name: "Renombrar «feature»" });
    const input = within(dialog).getByRole("textbox", { name: "Nombre" });
    expect(input).toHaveValue("feature");
    const submit = within(dialog).getByRole("button", { name: "Renombrar" });
    expect(submit).toBeDisabled();
    await user.clear(input);
    await user.type(input, "feature-2{Enter}");
    await waitFor(() =>
      expect(ipc.renameBranch).toHaveBeenCalledWith(1, "feature", "feature-2"),
    );
  });

  it("checks out a remote branch through a local tracking branch", async () => {
    const user = userEvent.setup();
    const refs = sampleRefs();
    refs.refs.push({
      name: "refs/remotes/origin/topic",
      shortName: "origin/topic",
      kind: "remote",
      target: oid("c"),
      remote: "origin",
      upstream: null,
      isHead: false,
      ahead: null,
      behind: null,
      upstreamGone: false,
    });
    ipc.createBranch.mockResolvedValue({
      status: "ok",
      data: { type: "switched" },
    });
    renderSidebar(refs);
    const remotes = screen
      .getByRole("button", { name: /Remotos/ })
      .closest("section") as HTMLElement;

    // origin/main is already tracked by main.
    await user.dblClick(within(remotes).getByRole("button", { name: "main" }));
    expect(ipc.checkoutBranch).toHaveBeenCalledWith(1, "main");

    await user.dblClick(within(remotes).getByRole("button", { name: "topic" }));
    await waitFor(() =>
      expect(ipc.createBranch).toHaveBeenCalledWith(
        1,
        "topic",
        "refs/remotes/origin/topic",
        true,
        true,
      ),
    );
  });
});
