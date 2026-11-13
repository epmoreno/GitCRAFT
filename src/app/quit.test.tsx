import { readFileSync } from "node:fs";
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../i18n";
import { useConflictStore } from "../features/conflicts/store";
import { ipc } from "../test/ipc";
import { SRC_DIR } from "../test/sources";
import { App } from "./App";
import { installAppMenu } from "./menu";
import { closeWindow } from "./windowClose";

/** The actions App gave the menu bar. */
function menuActions() {
  const call = vi.mocked(installAppMenu).mock.calls.at(-1);
  if (!call) throw new Error("the menu was not installed");
  return call[1];
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  ipc.restoreSession.mockResolvedValue({
    status: "ok",
    data: { opened: [], active: null, failed: [] },
  });
});

describe("quitting (⌘Q, or Quit in the menu)", () => {
  it("quits at once when no conflict choice is unsaved", async () => {
    render(<App />);
    act(() => menuActions().quit());
    expect(closeWindow).toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("asks first, listing the files, when some are", async () => {
    const user = userEvent.setup();
    useConflictStore
      .getState()
      .setChoice(1, "src/main.rs", "fp", 0, { kind: "ours" });
    render(<App />);
    act(() => menuActions().quit());
    const dialog = await screen.findByRole("alertdialog", {
      name: "Hay elecciones sin guardar",
    });
    expect(dialog).toHaveTextContent("Si sales de GitCRAFT, se pierden");
    expect(dialog).toHaveTextContent("src/main.rs");
    expect(closeWindow).not.toHaveBeenCalled();

    await user.click(
      within(dialog).getByRole("button", { name: "Salir de GitCRAFT" }),
    );
    expect(closeWindow).toHaveBeenCalled();
    expect(useConflictStore.getState().drafts).toEqual({});
  });
});

/*
 * The window-close guard uses Tauri's onCloseRequested: with a listener,
 * Tauri always holds the window open and the listener destroys it, which
 * needs this permission. Without it nothing closes the app (7565461). The
 * smoke run (smoke.ts) checks the real app closes; this catches it sooner.
 */
it("grants what closing the window needs", () => {
  const capability = JSON.parse(
    readFileSync(`${SRC_DIR}../src-tauri/capabilities/default.json`, "utf8"),
  ) as { permissions: string[] };
  expect(capability.permissions).toContain("core:window:allow-close");
  expect(capability.permissions).toContain("core:window:allow-destroy");
});
