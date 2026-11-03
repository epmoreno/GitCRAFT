import {
  Menu,
  MenuItem,
  PredefinedMenuItem,
  Submenu,
} from "@tauri-apps/api/menu";
import type { TFunction } from "i18next";

/**
 * The macOS menu bar. Replaces the default one, whose "Close Window" on ⌘W
 * would close the whole app: here ⌘W closes the active tab. ⌘T opens a new
 * tab with the start page.
 */
export async function installAppMenu(
  t: TFunction,
  actions: { newTab: () => void; closeTab: () => void },
) {
  const separator = () => PredefinedMenuItem.new({ item: "Separator" });
  const app = await Submenu.new({
    text: "GitCRAFT",
    items: [
      await PredefinedMenuItem.new({ item: { About: null } }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Services" }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Hide" }),
      await PredefinedMenuItem.new({ item: "HideOthers" }),
      await PredefinedMenuItem.new({ item: "ShowAll" }),
      await separator(),
      await PredefinedMenuItem.new({ item: "Quit" }),
    ],
  });
  const file = await Submenu.new({
    text: t("menu.file"),
    items: [
      await MenuItem.new({
        id: "new-tab",
        text: t("menu.newTab"),
        accelerator: "CmdOrCtrl+T",
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
        accelerator: "CmdOrCtrl+W",
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
