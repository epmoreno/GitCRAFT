import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { GhStatus, LoginEvent } from "../../ipc";
import { commandError, ipc } from "../../test/ipc";
import { setPlatformForTests } from "../../app/platform";
import { SettingsDialog } from "./SettingsDialog";
import { useSettingsStore } from "./store";

const status = (overrides: Partial<GhStatus> = {}): GhStatus => ({
  path: "/opt/homebrew/bin/gh",
  version: "gh version 2.62.0 (2024-11-14)",
  accounts: [{ login: "ana", active: true, valid: true, gitProtocol: "https" }],
  sessionUnreachable: false,
  githubHelpers: [],
  generalHelpers: ["osxkeychain"],
  gitUsesGh: false,
  ...overrides,
});

async function openGithub(initial: GhStatus = status()) {
  ipc.ghStatus.mockResolvedValue({ status: "ok", data: initial });
  render(<SettingsDialog />);
  act(() => useSettingsStore.getState().openSettings("github"));
  return screen.findByRole("region", { name: "Cuenta de GitHub" });
}

describe("GitHub account in Settings", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("explains how to install gh when it is missing", async () => {
    const user = userEvent.setup();
    const section = await openGithub(
      status({ path: null, version: null, accounts: [] }),
    );
    expect(
      await within(section).findByText("GitHub CLI (gh) no está instalado."),
    ).toBeVisible();
    expect(within(section).getByText("brew install gh")).toHaveClass(
      "selectable",
    );
    ipc.ghStatus.mockResolvedValue({ status: "ok", data: status() });
    await user.click(
      within(section).getByRole("button", { name: "Comprobar de nuevo" }),
    );
    expect(await within(section).findByText("@ana")).toBeVisible();
  });

  // P.6: the install hint follows the system's package manager.
  it.each([
    [
      "windows",
      "Instálalo con winget en la terminal:",
      "winget install --id GitHub.cli",
    ],
    [
      "linux",
      "Instálalo con el gestor de paquetes de tu distribución; por ejemplo, en Debian o Ubuntu:",
      "sudo apt install gh",
    ],
  ] as const)(
    "on %s, explains how to install gh there",
    async (system, text, command) => {
      setPlatformForTests(system);
      const section = await openGithub(
        status({ path: null, version: null, accounts: [] }),
      );
      expect(await within(section).findByText(text)).toBeVisible();
      expect(within(section).getByText(command)).toHaveClass("selectable");
      expect(section).not.toHaveTextContent("brew");
    },
  );

  it("shows who is signed in, and signs out after confirming", async () => {
    const user = userEvent.setup();
    const section = await openGithub();
    expect(await within(section).findByText("@ana")).toBeVisible();
    expect(section).toHaveTextContent("gh version 2.62.0");
    expect(section).toHaveTextContent("No guarda ningún token propio");
    expect(section).not.toHaveTextContent(/gh[opsu]_/);

    ipc.ghLogout.mockResolvedValue({
      status: "ok",
      data: status({ accounts: [] }),
    });
    await user.click(
      within(section).getByRole("button", { name: "Cerrar sesión" }),
    );
    const confirm = screen.getByRole("alertdialog", {
      name: "¿Cerrar la sesión de ana?",
    });
    expect(ipc.ghLogout).not.toHaveBeenCalled();
    await user.click(
      within(confirm).getByRole("button", { name: "Cerrar sesión" }),
    );
    expect(ipc.ghLogout).toHaveBeenCalledWith("ana");
    expect(
      await within(section).findByText("No hay sesión de GitHub."),
    ).toBeVisible();
  });

  it("signs in inside the app with the one-time code, and can cancel", async () => {
    const user = userEvent.setup();
    let send: ((event: LoginEvent) => void) | undefined;
    let finish:
      ((value: Awaited<ReturnType<typeof ipc.ghLogin>>) => void) | undefined;
    ipc.ghLogin.mockImplementation((onEvent) => {
      send = onEvent;
      return new Promise((resolve) => (finish = resolve));
    });
    const section = await openGithub(status({ accounts: [] }));
    await user.click(
      await within(section).findByRole("button", {
        name: "Iniciar sesión en GitHub",
      }),
    );
    expect(within(section).getByText("Iniciando sesión con gh…")).toBeVisible();
    act(() =>
      send?.({
        type: "code",
        code: "WXYZ-1234",
        url: "https://github.com/login/device",
      }),
    );
    expect(
      within(section).getByLabelText("Código de un solo uso"),
    ).toHaveTextContent("WXYZ-1234");
    expect(section).toHaveTextContent("https://github.com/login/device");

    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    await user.click(
      within(section).getByRole("button", { name: "Copiar código" }),
    );
    expect(writeText).toHaveBeenCalledWith("WXYZ-1234");

    await user.click(within(section).getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelGhLogin).toHaveBeenCalled();
    await act(async () =>
      finish?.({ status: "error", error: commandError("cancelled") }),
    );
    // Cancelling is not an error.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(
      within(section).getByRole("button", { name: "Iniciar sesión en GitHub" }),
    ).toBeVisible();
  });

  it("shows the account once the browser confirmed", async () => {
    const user = userEvent.setup();
    ipc.ghLogin.mockResolvedValue({ status: "ok", data: status() });
    const section = await openGithub(status({ accounts: [] }));
    await user.click(
      await within(section).findByRole("button", {
        name: "Iniciar sesión en GitHub",
      }),
    );
    expect(await within(section).findByText("@ana")).toBeVisible();
  });

  it("asks to sign in again when the token is no longer valid", async () => {
    const section = await openGithub(
      status({
        accounts: [
          { login: "ana", active: true, valid: false, gitProtocol: "https" },
        ],
      }),
    );
    expect(await within(section).findByText(/ya no es válida/)).toBeVisible();
    expect(
      within(section).getByRole("button", { name: "Iniciar sesión en GitHub" }),
    ).toBeVisible();
  });

  it("without a connection says it could not check, never that there is no session", async () => {
    const user = userEvent.setup();
    const section = await openGithub(
      status({ accounts: [], sessionUnreachable: true }),
    );
    expect(
      await within(section).findByText(
        "No hay conexión con GitHub. Comprueba tu red e inténtalo de nuevo.",
      ),
    ).toBeVisible();
    expect(section).not.toHaveTextContent("No hay sesión de GitHub.");
    expect(
      within(section).queryByRole("button", {
        name: "Iniciar sesión en GitHub",
      }),
    ).toBeNull();
    expect(
      within(section).queryByRole("button", { name: "Cerrar sesión" }),
    ).toBeNull();

    ipc.ghStatus.mockResolvedValue({ status: "ok", data: status() });
    await user.click(
      within(section).getByRole("button", { name: "Comprobar de nuevo" }),
    );
    expect(await within(section).findByText("@ana")).toBeVisible();
  });

  it("does not offer anything about the session while an account cannot be checked", async () => {
    const section = await openGithub(
      status({
        accounts: [
          { login: "ana", active: true, valid: false, gitProtocol: "https" },
        ],
        sessionUnreachable: true,
      }),
    );
    await within(section).findByText(/No hay conexión con GitHub/);
    expect(section).not.toHaveTextContent("ya no es válida");
    expect(
      within(section).queryByRole("button", { name: "Cerrar sesión" }),
    ).toBeNull();
  });

  it("explains what gh auth setup-git changes in ~/.gitconfig before running it", async () => {
    const user = userEvent.setup();
    ipc.ghSetupGit.mockResolvedValue({
      status: "ok",
      data: status({
        gitUsesGh: true,
        githubHelpers: ["", "!/opt/homebrew/bin/gh auth git-credential"],
      }),
    });
    const section = await openGithub();
    const git = await within(section).findByRole("group", {
      name: "Credenciales de Git para github.com",
    });
    expect(git).toHaveTextContent("Git usa ahora osxkeychain");
    await user.click(
      within(git).getByRole("button", {
        name: "Usar gh para las credenciales de Git…",
      }),
    );
    const confirm = screen.getByRole("alertdialog", {
      name: "¿Usar gh para las credenciales de Git?",
    });
    expect(confirm).toHaveTextContent('[credential "https://github.com"]');
    expect(confirm).toHaveTextContent(
      "helper = !/opt/homebrew/bin/gh auth git-credential",
    );
    expect(confirm).toHaveTextContent("git usa: osxkeychain");
    expect(confirm).toHaveTextContent(
      "--unset-all credential.https://github.com.helper",
    );
    expect(ipc.ghSetupGit).not.toHaveBeenCalled();
    await user.click(within(confirm).getByRole("button", { name: "Usar gh" }));
    expect(ipc.ghSetupGit).toHaveBeenCalled();
    expect(
      await within(section).findByText(
        "Git pide a gh las credenciales de github.com.",
      ),
    ).toBeVisible();
  });
});
