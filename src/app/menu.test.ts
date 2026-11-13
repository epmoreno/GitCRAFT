import i18n from "../i18n";
import { setPlatformForTests } from "./platform";

/*
 * The menu bar per system, built against a stand-in for Tauri's menu API
 * that records what is created. (setup.ts replaces ./menu for every other
 * test; here the real module is loaded.)
 */
type Node = {
  id?: string;
  action?: () => void;
  text?: string;
  item?: unknown;
  accelerator?: string;
  items?: Node[];
};

const built: { menu: Node[] } = { menu: [] };

vi.mock("@tauri-apps/api/menu", () => {
  const make = (options: Node) => Promise.resolve(options);
  return {
    Menu: {
      new: (options: { items: Node[] }) => {
        built.menu = options.items;
        return Promise.resolve({ setAsAppMenu: () => Promise.resolve() });
      },
    },
    MenuItem: { new: make },
    PredefinedMenuItem: { new: make },
    Submenu: { new: make },
  };
});

const actions = {
  openSettings: vi.fn(),
  newTab: vi.fn(),
  closeTab: vi.fn(),
  quit: vi.fn(),
};

/** Each submenu as "Title: item, item…" (separators as "—"). */
function outline(): string[] {
  const label = (n: Node) =>
    n.text ??
    (typeof n.item === "string"
      ? n.item === "Separator"
        ? "—"
        : n.item
      : Object.keys(n.item as object)[0]!);
  return built.menu.map(
    (sub) =>
      `${sub.text}: ${(sub.items ?? []).map((n) => (n.accelerator ? `${label(n)} [${n.accelerator}]` : label(n))).join(", ")}`,
  );
}

async function install() {
  const { installAppMenu } =
    await vi.importActual<typeof import("./menu")>("./menu");
  await installAppMenu(i18n.t.bind(i18n), actions);
}

describe("menu bar", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("on macOS, is the one it always was", async () => {
    await install();
    expect(outline()).toEqual([
      "GitCRAFT: About, —, Ajustes… [CmdOrCtrl+,], —, Services, —, Hide, HideOthers, ShowAll, —, Quit GitCRAFT [CmdOrCtrl+Q]",
      "Archivo: Nueva pestaña [CmdOrCtrl+T]",
      "Edición: Undo, Redo, —, Cut, Copy, Paste, SelectAll",
      "Ventana: Cerrar pestaña [CmdOrCtrl+W], —, Minimize, Fullscreen",
    ]);
  });

  // P.9: a bar inside the window, without the macOS items.
  it.each(["windows", "linux"] as const)(
    "on %s, has File, Edit and Help with the same accelerators",
    async (system) => {
      setPlatformForTests(system);
      await install();
      expect(outline()).toEqual([
        "Archivo: Nueva pestaña [CmdOrCtrl+T], Cerrar pestaña [CmdOrCtrl+W], —, Ajustes… [CmdOrCtrl+,], —, Salir",
        "Edición: Cortar, Copiar, Pegar, —, Seleccionar todo",
        "Ayuda: Acerca de GitCRAFT",
      ]);
    },
  );

  // Quitting goes through the app (warning about unsaved conflict choices),
  // never straight to the system.
  it.each(["mac", "windows", "linux"] as const)(
    "on %s, Quit is the app's own action",
    async (system) => {
      setPlatformForTests(system);
      await install();
      const quit = built.menu
        .flatMap((sub) => sub.items ?? [])
        .find((n) => n.id === "quit");
      expect(quit?.item).toBeUndefined();
      quit?.action?.();
      expect(actions.quit).toHaveBeenCalled();
    },
  );
});
