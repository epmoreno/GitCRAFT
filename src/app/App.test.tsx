import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../i18n";
import type { RepoChanged } from "../ipc";
import { commandError, ipc, opened, sampleRefs } from "../test/ipc";
import { App } from "./App";

describe("App", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("renders the three zones", () => {
    render(<App />);
    expect(
      screen.getByRole("complementary", { name: "Barra lateral" }),
    ).toBeVisible();
    expect(screen.getByRole("main", { name: "Historial" })).toBeVisible();
    expect(
      screen.getByRole("complementary", { name: "Detalle" }),
    ).toBeVisible();
  });

  it("explains why repository actions are disabled", () => {
    render(<App />);
    for (const label of ["Pull", "Push", "Rama", "Stash", "Deshacer"]) {
      const button = screen.getByRole("button", { name: label });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription("Abre un repositorio primero");
    }
  });

  it("shows the start page with an open button and an empty recent list", async () => {
    render(<App />);
    const main = screen.getByRole("main");
    expect(
      await within(main).findByText("Ningún repositorio abierto"),
    ).toBeVisible();
    expect(
      within(main).getByRole("button", { name: "Abrir repositorio…" }),
    ).toBeEnabled();
    expect(
      await within(main).findByText("Aún no has abierto ningún repositorio."),
    ).toBeVisible();
    expect(
      within(main).getByRole("button", { name: "Inicializar repositorio…" }),
    ).toBeEnabled();
  });

  it("opens a recent repository in a tab", async () => {
    const user = userEvent.setup();
    ipc.listRecentRepositories.mockResolvedValue({
      status: "ok",
      data: [{ path: "/src/gitcraft", name: "gitcraft", lastOpened: 1 }],
    });
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(1, "/src/gitcraft"),
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /gitcraft/ }));

    expect(ipc.openRepository).toHaveBeenCalledWith("/src/gitcraft");
    expect(
      await screen.findByRole("tab", { name: "gitcraft" }),
    ).toHaveAttribute("aria-selected", "true");
    const main = screen.getByRole("main");
    expect(
      await within(main).findByRole("listbox", {
        name: "Historial de gitcraft",
      }),
    ).toBeVisible();
    expect(ipc.streamGraph).toHaveBeenCalledWith(1, expect.any(Function));
    // The path is content the user may want to copy.
    expect(within(main).getByText("/src/gitcraft")).toHaveClass("selectable");
    expect(
      screen.getByRole("button", { name: "Deshacer" }),
    ).toHaveAccessibleDescription("Todavía no disponible");
  });

  it("opens the folder chosen in the picker", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/work/project");
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(7, "/work/project"),
    });
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Abrir repositorio…" }),
    );

    expect(ipc.pickFolder).toHaveBeenCalledWith("Abrir repositorio");
    expect(await screen.findByRole("tab", { name: "project" })).toBeVisible();
  });

  it("explains that a plain folder is not a repository", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/Users/ana/Fotos");
    ipc.openRepository.mockResolvedValue({
      status: "error",
      error: commandError(
        "notARepository",
        "$ git rev-parse --show-toplevel --absolute-git-dir\nfatal: not a git repository (or any of the parent directories): .git",
      ),
    });
    render(<App />);

    await user.click(
      await screen.findByRole("button", { name: "Abrir repositorio…" }),
    );

    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText("No se pudo abrir el repositorio"),
    ).toBeVisible();
    expect(
      within(alert).getByText(
        /La carpeta «\/Users\/ana\/Fotos» no es un repositorio Git/,
      ),
    ).toBeVisible();

    await user.click(within(alert).getByText("Ver la salida original de Git"));
    const raw = within(alert).getByText(/fatal: not a git repository/);
    expect(raw).toBeVisible();
    expect(raw).toHaveClass("selectable");

    expect(screen.queryByRole("tab")).toBeNull();
    await user.click(within(alert).getByRole("button", { name: "Cerrar" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("closes a tab and returns to the start page", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/a/repo");
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(1, "/a/repo"),
    });
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Abrir repositorio…" }),
    );
    await screen.findByRole("tab", { name: "repo" });

    await user.click(screen.getByRole("button", { name: "Cerrar repo" }));

    expect(ipc.closeRepository).toHaveBeenCalledWith(1);
    expect(screen.queryByRole("tab")).toBeNull();
    expect(screen.getByText("Ningún repositorio abierto")).toBeVisible();
  });

  it("refreshes the graph when the repository changes outside the app", async () => {
    const user = userEvent.setup();
    let notify: ((change: RepoChanged) => void) | undefined;
    ipc.onRepoChanged.mockImplementation(async (handler) => {
      notify = handler;
      return () => {};
    });
    ipc.pickFolder.mockResolvedValue("/a/repo");
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(1, "/a/repo"),
    });
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "Abrir repositorio…" }),
    );
    await screen.findByText("0 commits");
    expect(ipc.streamGraph).toHaveBeenCalledTimes(1);

    const statusCalls = ipc.workingStatus.mock.calls.length;
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    act(() => notify?.({ id: 1, refs: true, worktree: true, stash: false }));
    await waitFor(() => expect(ipc.streamGraph).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(ipc.workingStatus.mock.calls.length).toBeGreaterThan(statusCalls),
    );

    // A file edit only refreshes the status, not the history.
    const before = ipc.workingStatus.mock.calls.length;
    act(() => notify?.({ id: 1, refs: false, worktree: true, stash: false }));
    await waitFor(() =>
      expect(ipc.workingStatus.mock.calls.length).toBeGreaterThan(before),
    );
    expect(ipc.streamGraph).toHaveBeenCalledTimes(2);
  });

  it("switches language", async () => {
    await i18n.changeLanguage("en");
    render(<App />);
    expect(await screen.findByText("No repository open")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Pull" }),
    ).toHaveAccessibleDescription("Open a repository first");
  });
});
