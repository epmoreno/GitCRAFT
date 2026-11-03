import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { Toolbar } from "../../app/Toolbar";
import { defaultGeneralSettings, ipc } from "../../test/ipc";
import { SettingsDialog } from "./SettingsDialog";
import { useSettingsStore } from "./store";

function renderSettings() {
  render(
    <>
      <Toolbar />
      <SettingsDialog />
    </>,
  );
}

const dialog = () => screen.getByRole("dialog", { name: "Ajustes" });

describe("settings", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });
  afterEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("opens from the gear and closes with Escape", async () => {
    const user = userEvent.setup();
    renderSettings();
    const gear = screen.getByRole("button", { name: "Ajustes" });
    expect(gear).toHaveAccessibleDescription("Ajustes (⌘,)");
    await user.click(gear);
    expect(
      within(dialog()).getByRole("button", { name: "Generales" }),
    ).toHaveAttribute("aria-current", "page");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("applies the saved language and text size at start", async () => {
    ipc.generalSettings.mockResolvedValue({
      status: "ok",
      data: { ...defaultGeneralSettings(), language: "en", textSize: "larger" },
    });
    await act(() => useSettingsStore.getState().load());
    expect(i18n.language).toBe("en");
    expect(ipc.setUiZoom).toHaveBeenCalledWith(1.3);
  });

  it("keeps the detected language at start when none was chosen", async () => {
    await act(() => useSettingsStore.getState().load());
    expect(i18n.language).toBe("es");
    expect(ipc.setUiZoom).toHaveBeenCalledWith(1);
  });

  it("changes the language at once and saves it", async () => {
    const user = userEvent.setup();
    renderSettings();
    act(() => useSettingsStore.getState().openSettings());
    await user.selectOptions(
      within(dialog()).getByRole("combobox", { name: "Idioma" }),
      "en",
    );
    expect(i18n.language).toBe("en");
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeVisible();
    expect(ipc.setGeneralSettings).toHaveBeenLastCalledWith({
      ...defaultGeneralSettings(),
      language: "en",
    });
  });

  it("scales the whole UI with the text size", async () => {
    const user = userEvent.setup();
    renderSettings();
    act(() => useSettingsStore.getState().openSettings());
    await user.click(within(dialog()).getByRole("radio", { name: "Grande" }));
    expect(ipc.setUiZoom).toHaveBeenLastCalledWith(1.15);
    expect(ipc.setGeneralSettings).toHaveBeenLastCalledWith({
      ...defaultGeneralSettings(),
      textSize: "large",
    });
    expect(
      within(dialog()).getByRole("radio", { name: "Grande" }),
    ).toBeChecked();
  });

  it("chooses a default folder for clones, or goes back to the last one used", async () => {
    const user = userEvent.setup();
    ipc.pickFolder.mockResolvedValue("/Users/ana/Código");
    renderSettings();
    act(() => useSettingsStore.getState().openSettings());
    const field = within(dialog()).getByRole("group", {
      name: "Carpeta por defecto para clonar",
    });
    expect(field).toHaveTextContent("La última que usaste");
    await user.click(within(field).getByRole("button", { name: "Elegir…" }));
    expect(field).toHaveTextContent("/Users/ana/Código");
    expect(ipc.setGeneralSettings).toHaveBeenLastCalledWith({
      ...defaultGeneralSettings(),
      cloneFolder: "/Users/ana/Código",
    });
    await user.click(
      within(field).getByRole("button", { name: "Usar la última" }),
    );
    expect(field).toHaveTextContent("La última que usaste");
  });

  it("opens from the app menu (⌘,)", async () => {
    const { App } = await import("../../app/App");
    render(<App />);
    const { installAppMenu } = await import("../../app/menu");
    const [, actions] = vi.mocked(installAppMenu).mock.lastCall!;
    act(() => actions.openSettings());
    expect(dialog()).toBeVisible();
  });
});
