import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { IdentityInfo } from "../../ipc";
import { ipc, opened } from "../../test/ipc";
import { useRepoStore } from "../repo/store";
import { plausibleEmail } from "./IdentitySection";
import { SettingsDialog } from "./SettingsDialog";
import { useSettingsStore } from "./store";

const info = (overrides: Partial<IdentityInfo> = {}): IdentityInfo => ({
  global: { name: "Ana", email: "ana@casa.es" },
  repo: { name: null, email: "ana@trabajo.es" },
  effectiveName: {
    value: "Ana",
    scope: "global",
    origin: "file:/Users/ana/.gitconfig",
  },
  effectiveEmail: {
    value: "ana@trabajo.es",
    scope: "local",
    origin: "file:.git/config",
  },
  ...overrides,
});

async function openIdentity() {
  useRepoStore.setState({ tabs: [opened(3, "/r/web")], activeId: 3 });
  render(<SettingsDialog />);
  act(() => useSettingsStore.getState().openSettings("identity"));
  return screen.findByRole("group", { name: "Global" });
}

describe("git identity in Settings", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.gitIdentity.mockResolvedValue({ status: "ok", data: info() });
  });

  it("shows the global identity, the repository's own and where each part comes from", async () => {
    const global = await openIdentity();
    expect(ipc.gitIdentity).toHaveBeenCalledWith(3);
    expect(within(global).getByRole("textbox", { name: "Nombre" })).toHaveValue(
      "Ana",
    );
    const repo = screen.getByRole("group", { name: "Este repositorio: web" });
    const name = within(repo).getByRole("textbox", { name: "Nombre" });
    expect(name).toHaveValue("");
    expect(name).toHaveAttribute("placeholder", "Ana");
    expect(
      screen.getByText("Ana <ana@trabajo.es>").closest("p"),
    ).toHaveTextContent(
      "Los commits de web se firman como Ana <ana@trabajo.es>",
    );
    expect(
      screen.getByText("Nombre: de la configuración global"),
    ).toBeVisible();
    expect(screen.getByText("Email: de este repositorio")).toBeVisible();
  });

  it("saves the global identity after showing the commands and the old values", async () => {
    const user = userEvent.setup();
    ipc.identityChanges.mockResolvedValue({
      status: "ok",
      data: [
        { key: "user.name", value: "Ana Pérez" },
        { key: "user.email", value: "ana@casa.es" },
      ],
    });
    ipc.setGitIdentity.mockResolvedValue({ status: "ok", data: info() });
    const global = await openIdentity();
    const name = within(global).getByRole("textbox", { name: "Nombre" });
    await user.clear(name);
    await user.type(name, "Ana Pérez");
    await user.click(within(global).getByRole("button", { name: "Guardar" }));

    const confirm = screen.getByRole("alertdialog", {
      name: "¿Cambiar la configuración global de git?",
    });
    // Only what changes is listed.
    const rows = within(confirm).getAllByRole("listitem");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent(
      "git config --global user.name Ana Pérez",
    );
    expect(rows[0]).toHaveTextContent("Valor anterior: Ana");
    await user.click(within(confirm).getByRole("button", { name: "Cambiar" }));
    expect(ipc.setGitIdentity).toHaveBeenCalledWith(null, {
      name: "Ana Pérez",
      email: "ana@casa.es",
    });
  });

  it("goes back to the global identity in a repository", async () => {
    const user = userEvent.setup();
    ipc.identityChanges.mockResolvedValue({
      status: "ok",
      data: [
        { key: "user.name", value: null },
        { key: "user.email", value: null },
      ],
    });
    ipc.setGitIdentity.mockResolvedValue({ status: "ok", data: info() });
    await openIdentity();
    const repo = screen.getByRole("group", { name: "Este repositorio: web" });
    await user.click(
      within(repo).getByRole("button", { name: "Usar la global" }),
    );
    const confirm = screen.getByRole("alertdialog", {
      name: "¿Cambiar la configuración de este repositorio?",
    });
    expect(within(confirm).getAllByRole("listitem")).toHaveLength(1);
    expect(confirm).toHaveTextContent(
      "git config --local --unset-all user.email",
    );
    expect(confirm).toHaveTextContent("Valor anterior: ana@trabajo.es");
    await user.click(within(confirm).getByRole("button", { name: "Cambiar" }));
    expect(ipc.setGitIdentity).toHaveBeenCalledWith(3, {
      name: null,
      email: null,
    });
  });

  it("refuses an email that does not look like one", async () => {
    const user = userEvent.setup();
    const global = await openIdentity();
    const email = within(global).getByRole("textbox", { name: "Email" });
    await user.clear(email);
    await user.type(email, "ana en casa");
    expect(email).toHaveAttribute("aria-invalid", "true");
    expect(
      within(global).getByText("Ese email no parece válido."),
    ).toBeVisible();
    expect(
      within(global).getByRole("button", { name: "Guardar" }),
    ).toBeDisabled();
    expect(plausibleEmail("")).toBe(true);
    expect(plausibleEmail("a@b")).toBe(true);
    expect(plausibleEmail("a@@b")).toBe(false);
  });

  it("shows only the global identity without an open repository", async () => {
    ipc.gitIdentity.mockResolvedValue({
      status: "ok",
      data: info({ repo: null }),
    });
    render(<SettingsDialog />);
    act(() => useSettingsStore.getState().openSettings("identity"));
    expect(await screen.findByRole("group", { name: "Global" })).toBeVisible();
    expect(ipc.gitIdentity).toHaveBeenCalledWith(null);
    expect(screen.queryByText(/Este repositorio/)).not.toBeInTheDocument();
  });
});
