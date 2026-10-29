import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { InitCheck } from "../../ipc";
import { commandError, ipc, opened } from "../../test/ipc";
import { useRepoStore } from "../repo/store";
import { StartPage } from "../repo/StartPage";
import { CreateDialogs } from "./CreateDialogs";

const check = (overrides: Partial<InitCheck> = {}): InitCheck => ({
  defaultBranch: "main",
  alreadyRepository: false,
  inside: null,
  empty: true,
  ...overrides,
});

function renderStart() {
  useRepoStore.setState({ restored: true });
  render(
    <>
      <StartPage />
      <CreateDialogs />
    </>,
  );
}

describe("initialize repository", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("creates a repository with the chosen first branch and opens it", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/nuevo");
    ipc.checkInit.mockResolvedValue({
      status: "ok",
      data: check({ defaultBranch: "trunk", empty: false }),
    });
    ipc.initRepository.mockResolvedValue({
      status: "ok",
      data: opened(9, "/nuevo"),
    });
    renderStart();
    await user.click(
      screen.getByRole("button", { name: "Inicializar repositorio…" }),
    );
    expect(ipc.checkInit).toHaveBeenCalledWith("/nuevo");

    const dialog = await screen.findByRole("dialog", {
      name: "Inicializar repositorio",
    });
    const input = within(dialog).getByRole("textbox", { name: "Rama inicial" });
    expect(input).toHaveValue("trunk");
    expect(
      within(dialog).getByText(/quedarán como archivos sin seguimiento/),
    ).toBeVisible();

    await user.clear(input);
    await user.type(input, "principal");
    await user.click(
      within(dialog).getByRole("button", { name: "Crear repositorio" }),
    );
    expect(ipc.initRepository).toHaveBeenCalledWith("/nuevo", "principal");
    await waitFor(() => expect(useRepoStore.getState().activeId).toBe(9));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("rejects invalid branch names and warns about nesting", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/repo/sub");
    ipc.checkInit.mockResolvedValue({
      status: "ok",
      data: check({ inside: "/repo" }),
    });
    ipc.validBranchName.mockImplementation(async (_path, name) => ({
      status: "ok",
      data: !name.includes(".."),
    }));
    renderStart();
    await user.click(
      screen.getByRole("button", { name: "Inicializar repositorio…" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(/dentro del repositorio «\/repo»/),
    ).toBeVisible();
    const input = within(dialog).getByRole("textbox");
    await user.clear(input);
    await user.type(input, "a..b");
    expect(await within(dialog).findByText(/no es válido/)).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "Crear repositorio" }),
    ).toBeDisabled();
    expect(ipc.initRepository).not.toHaveBeenCalled();
  });

  it("offers to open a folder that already is a repository", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/ya");
    ipc.checkInit.mockResolvedValue({
      status: "ok",
      data: check({ alreadyRepository: true }),
    });
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(3, "/ya"),
    });
    renderStart();
    await user.click(
      screen.getByRole("button", { name: "Inicializar repositorio…" }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Ya es un repositorio",
    });
    await user.click(within(dialog).getByRole("button", { name: "Abrirlo" }));
    expect(ipc.openRepository).toHaveBeenCalledWith("/ya");
    expect(ipc.initRepository).not.toHaveBeenCalled();
  });

  it("shows why it failed and keeps the dialog", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/x");
    ipc.checkInit.mockResolvedValue({ status: "ok", data: check() });
    ipc.initRepository.mockResolvedValue({
      status: "error",
      error: commandError("alreadyARepository"),
    });
    renderStart();
    await user.click(
      screen.getByRole("button", { name: "Inicializar repositorio…" }),
    );
    await user.click(
      await screen.findByRole("button", { name: "Crear repositorio" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "«/x» ya es un repositorio Git.",
    );
    expect(screen.getByRole("dialog")).toBeVisible();
  });
});
