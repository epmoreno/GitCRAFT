import {
  Menu,
  MenuItem,
  PredefinedMenuItem,
  Submenu,
} from "@tauri-apps/api/menu";
import type { TFunction } from "i18next";
import { isMac } from "./platform";
import { accelerator } from "./shortcuts";

type MenuActions = {
  openSettings: () => void;
  newTab: () => void;
  closeTab: () => void;
  /** Quit the app: asks first if conflict choices are unsaved. */
  quit: () => void;
};

/**
 * The menu bar: the macOS one, or on Windows and Linux a bar inside the
 * window (File, Edit, Help). Both take their accelerators from the shortcut
 * registry, so ⌘T / Ctrl+T, ⌘W / Ctrl+W and ⌘, / Ctrl+, work the same.
 */
export async function installAppMenu(t: TFunction, actions: MenuActions) {
  return isMac() ? macMenu(t, actions) : windowsAndLinuxMenu(t, actions);
}

/**
 * The macOS menu bar. Replaces the default one, whose "Close Window" on ⌘W
 * would close the whole app: here ⌘W closes the active tab. ⌘T opens a new
 * tab with the start page.
 */
async function macMenu(t: TFunction, actions: MenuActions) {
  const separator = () => PredefinedMenuItem.new({ item: "Separator" });
  const app = await Submenu.new({
    text: "GitCRAFT",
    items: [
      await PredefinedMenuItem.new({
        item: {
          // `credits` is what macOS shows; `comments`, Windows and Linux.
          About: {
            credits: t("app.description"),
            comments: t("app.description"),
          },
        },
      }),
      await separator(),
      await MenuItem.new({
        id: "settings",
        text: t("menu.settings"),
        accelerator: accelerator("settings"),
        action: actions.openSettings,
      }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Services" }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Hide" }),
      await PredefinedMenuItem.new({ item: "HideOthers" }),
      await PredefinedMenuItem.new({ item: "ShowAll" }),
      await separator(),
      // Not the predefined Quit: that one ends the app straight away
      // (AppKit's terminate:), without the warning about unsaved conflict
      // choices. Same place, text and ⌘Q; the text stays in English like
      // the other items macOS names (Services, Hide…).
      await MenuItem.new({
        id: "quit",
        text: t("menu.quitMac"),
        accelerator: accelerator("quit"),
        action: actions.quit,
      }),
    ],
  });
  const file = await Submenu.new({
    text: t("menu.file"),
    items: [
      await MenuItem.new({
        id: "new-tab",
        text: t("menu.newTab"),
        accelerator: accelerator("newTab"),
        action: actions.newTab,
      }),
    ],
  });
  // Copy and paste in text fields need the Edit menu's standard items.
  const edit = await Submenu.new({
    text: t("menu.edit"),
    items: [
      await PredefinedMenuItem.new({ item: "Undo" }),
      await PredefinedMenuItem.new({ item: "Redo" }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Cut" }),
      await PredefinedMenuItem.new({ item: "Copy" }),
      await PredefinedMenuItem.new({ item: "Paste" }),
      await PredefinedMenuItem.new({ item: "SelectAll" }),
    ],
  });
  const window = await Submenu.new({
    text: t("menu.window"),
    items: [
      await MenuItem.new({
        id: "close-tab",
        text: t("menu.closeTab"),
        accelerator: accelerator("closeTab"),
        action: actions.closeTab,
      }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Minimize" }),
      await PredefinedMenuItem.new({ item: "Fullscreen" }),
    ],
  });
  const menu = await Menu.new({ items: [app, file, edit, window] });
  await menu.setAsAppMenu();
}

/**
 * Windows and Linux: File, Edit and Help, without the macOS items (Services,
 * Hide…). Undo and Redo are left out: the menu items only work on macOS,
 * and Ctrl+Z / Ctrl+Y work in text fields anyway.
 */
async function windowsAndLinuxMenu(t: TFunction, actions: MenuActions) {
  const separator = () => PredefinedMenuItem.new({ item: "Separator" });
  const file = await Submenu.new({
    text: t("menu.file"),
    items: [
      await MenuItem.new({
        id: "new-tab",
        text: t("menu.newTab"),
        accelerator: accelerator("newTab"),
        action: actions.newTab,
      }),
      await MenuItem.new({
        id: "close-tab",
        text: t("menu.closeTab"),
        accelerator: accelerator("closeTab"),
        action: actions.closeTab,
      }),
      await separator(),
      await MenuItem.new({
        id: "settings",
        text: t("menu.settings"),
        accelerator: accelerator("settings"),
        action: actions.openSettings,
      }),
      await separator(),
      // The predefined Quit does nothing on Linux: close the window instead
      // (after the warning about unsaved conflict choices, if any).
      await MenuItem.new({
        id: "quit",
        text: t("menu.quit"),
        action: actions.quit,
      }),
    ],
  });
  const edit = await Submenu.new({
    text: t("menu.edit"),
    items: [
      await PredefinedMenuItem.new({ item: "Cut", text: t("menu.cut") }),
      await PredefinedMenuItem.new({ item: "Copy", text: t("menu.copy") }),
      await PredefinedMenuItem.new({ item: "Paste", text: t("menu.paste") }),
      await separator(),
      await PredefinedMenuItem.new({
        item: "SelectAll",
        text: t("menu.selectAll"),
      }),
    ],
  });
  const help = await Submenu.new({
    text: t("menu.help"),
    items: [
      await PredefinedMenuItem.new({
        text: t("menu.about"),
        item: { About: { comments: t("app.description") } },
      }),
    ],
  });
  const menu = await Menu.new({ items: [file, edit, help] });
  await menu.setAsAppMenu();
}
