import { act, render, screen, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import i18n from "../i18n";
import { SRC_DIR, srcFiles } from "../test/sources";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { useSettingsStore } from "../features/settings/store";
import en from "../i18n/en.json";
import es from "../i18n/es.json";
import { setPlatformForTests } from "./platform";
import {
  chordLabel,
  chordsOf,
  isShortcut,
  shortcutLabel,
  SHORTCUT_IDS,
  SHORTCUTS,
  type ShortcutId,
  whichShortcut,
} from "./shortcuts";

const key = (key: string, mods: Partial<KeyboardEvent> = {}) => ({
  key,
  metaKey: false,
  ctrlKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("shortcut registry", () => {
  it("matches exactly its keys and modifiers", () => {
    expect(
      isShortcut("changesDiscard", key("Backspace", { metaKey: true })),
    ).toBe(true);
    expect(isShortcut("changesDiscard", key("Backspace"))).toBe(false);
    expect(isShortcut("changesDiscard", key("Delete"))).toBe(false);
    expect(isShortcut("newTab", key("T", { metaKey: true }))).toBe(true);
    expect(isShortcut("tabByNumber", key("7", { metaKey: true }))).toBe(true);
    expect(isShortcut("tabByNumber", key("0", { metaKey: true }))).toBe(false);
    expect(isShortcut("nextTab", key("Tab", { ctrlKey: true }))).toBe(true);
    expect(
      isShortcut("nextTab", key("Tab", { ctrlKey: true, shiftKey: true })),
    ).toBe(false);
    expect(
      isShortcut("previousTab", key("Tab", { ctrlKey: true, shiftKey: true })),
    ).toBe(true);
    expect(isShortcut("commit", key("Enter", { ctrlKey: true }))).toBe(true);
    // A modifier that is not part of it never matches.
    expect(isShortcut("changesNext", key("ArrowDown", { metaKey: true }))).toBe(
      false,
    );
    expect(whichShortcut(["changesOpen", "changesStage"], key(" "))).toBe(
      "changesStage",
    );
  });

  it("writes chords the Mac way", () => {
    const named = (k: string) => `<${k}>`;
    const label = (id: ShortcutId) =>
      chordsOf(id).map((c) => chordLabel(c, named));
    expect(label("changesDiscard")).toEqual(["⌘⌫"]);
    expect(label("previousTab")).toEqual(["⌃⇧⇥"]);
    expect(label("tabByNumber")).toEqual(["⌘1…9"]);
    expect(label("changesStage")).toEqual(["< >"]);
    expect(label("commit")).toEqual(["⌘↩", "⌃↩"]);
    expect(shortcutLabel("tabByNumber", named, "3")).toBe("⌘3");
  });

  it("keeps the macOS hints exactly as before P.4", async () => {
    const named = (k: string) => i18n.t(`settings.shortcuts.key.${k}`);
    const hint = (text: string, id: ShortcutId, k?: string) =>
      i18n.t(text, { shortcut: shortcutLabel(id, named, k), path: "/r" });
    await i18n.changeLanguage("es");
    expect(hint("tabs.closeTab", "closeTab")).toBe("Cerrar pestaña (⌘W)");
    expect(hint("tabs.newTabHint", "newTab")).toBe("Nueva pestaña (⌘T)");
    expect(hint("tabs.titleWithShortcut", "tabByNumber", "2")).toBe("/r\n⌘2");
    expect(hint("commit.shortcut", "commit")).toBe("⌘↩ para confirmar");
    expect(hint("settings.openHint", "settings")).toBe("Ajustes (⌘,)");
    await i18n.changeLanguage("en");
    expect(hint("tabs.closeTab", "closeTab")).toBe("Close tab (⌘W)");
    expect(hint("commit.shortcut", "commit")).toBe("⌘↩ to confirm");
    await i18n.changeLanguage("es");
  });

  // P.4: what Windows and Linux use instead.
  describe("outside macOS", () => {
    beforeEach(() => setPlatformForTests("windows"));

    it("uses Ctrl where macOS uses ⌘", () => {
      expect(isShortcut("newTab", key("t", { ctrlKey: true }))).toBe(true);
      expect(isShortcut("newTab", key("t", { metaKey: true }))).toBe(false);
      expect(isShortcut("closeTab", key("w", { ctrlKey: true }))).toBe(true);
      expect(isShortcut("settings", key(",", { ctrlKey: true }))).toBe(true);
      expect(isShortcut("tabByNumber", key("4", { ctrlKey: true }))).toBe(true);
      expect(isShortcut("commit", key("Enter", { ctrlKey: true }))).toBe(true);
      expect(isShortcut("commit", key("Enter", { metaKey: true }))).toBe(false);
      expect(isShortcut("nextTab", key("Tab", { ctrlKey: true }))).toBe(true);
    });

    it("discards with Ctrl+Delete, not Backspace", () => {
      expect(
        isShortcut("changesDiscard", key("Delete", { ctrlKey: true })),
      ).toBe(true);
      expect(
        isShortcut("changesDiscard", key("Backspace", { ctrlKey: true })),
      ).toBe(false);
      expect(
        isShortcut("changesDiscard", key("Backspace", { metaKey: true })),
      ).toBe(false);
    });

    it("writes chords the Windows and Linux way", () => {
      const named = (k: string) => `<${k}>`;
      const label = (id: ShortcutId) =>
        chordsOf(id).map((c) => chordLabel(c, named));
      expect(label("changesDiscard")).toEqual(["Ctrl+<Delete>"]);
      expect(label("previousTab")).toEqual(["Ctrl+<Shift>+<Tab>"]);
      expect(label("tabByNumber")).toEqual(["Ctrl+1…9"]);
      expect(label("commit")).toEqual(["Ctrl+<Enter>"]);
      expect(label("newTab")).toEqual(["Ctrl+T"]);
      expect(label("changesNext")).toEqual(["↓"]);
      expect(shortcutLabel("tabByNumber", named, "3")).toBe("Ctrl+3");
    });

    it("lists them so in Settings → Shortcuts", async () => {
      await i18n.changeLanguage("es");
      render(<SettingsDialog />);
      act(() => useSettingsStore.getState().openSettings("shortcuts"));
      const section = screen.getByRole("region", { name: "Atajos" });
      expect(
        within(section).getByRole("row", {
          name: /Descartar el archivo sin preparar/,
        }),
      ).toHaveTextContent("Ctrl+Supr");
      expect(
        within(section).getByRole("row", { name: /Pestaña anterior/ }),
      ).toHaveTextContent("Ctrl+Mayús+Tab");
      expect(section).not.toHaveTextContent("⌘");
    });
  });

  it("no text writes a modifier by hand", () => {
    for (const [lang, texts] of [
      ["es", es],
      ["en", en],
    ] as const) {
      const flat = JSON.stringify(texts);
      expect(flat, lang).not.toMatch(/[⌘⌃⌥⇧]|Ctrl\+/);
    }
  });

  it("has a description for every shortcut in both languages", () => {
    for (const id of SHORTCUT_IDS) {
      expect(es.settings.shortcuts.action[id], id).toBeTruthy();
      expect(en.settings.shortcuts.action[id], id).toBeTruthy();
    }
  });

  it("is what Settings → Shortcuts lists, every one of them", async () => {
    await i18n.changeLanguage("es");
    render(<SettingsDialog />);
    act(() => useSettingsStore.getState().openSettings("shortcuts"));
    const section = screen.getByRole("region", { name: "Atajos" });
    const rows = within(section).getAllByRole("row");
    expect(rows).toHaveLength(SHORTCUT_IDS.length);
    const discard = within(section).getByRole("row", {
      name: /Descartar el archivo sin preparar/,
    });
    expect(discard).toHaveTextContent("⌘⌫");
    expect(
      within(section).getByRole("row", { name: /Nueva pestaña/ }),
    ).toHaveTextContent("⌘T");
    expect(
      within(section).getByRole("row", { name: /Preparar o quitar/ }),
    ).toHaveTextContent("Espacio");
  });
});

/*
 * The registry is the only place that knows keys: a handler comparing keys
 * or modifiers on its own would add a shortcut the list does not show, and
 * a literal accelerator in the menu would do the same.
 */
describe("no shortcut outside the registry", () => {
  const sources = srcFiles()
    .filter((p) => /\.(ts|tsx)$/.test(p) && !/\.test\.tsx?$/.test(p))
    .filter((p) => !p.startsWith("test/") && p !== "app/shortcuts.ts")
    .map((p) => [p, readFileSync(`${SRC_DIR}${p}`, "utf8")] as const);

  it("finds the sources", () => {
    expect(sources.map(([p]) => p)).toContain(
      "features/workdir/ChangeList.tsx",
    );
  });

  it("no handler compares keys or modifiers itself", () => {
    const pattern =
      /\.(metaKey|ctrlKey|shiftKey|altKey)\b|\b(event|e)\.key\s*[!=]==|case\s+"(Arrow\w+|Enter|Escape|Home|End|Tab|Backspace|Delete| )"/;
    const offenders = sources.flatMap(([path, text]) =>
      text
        .split("\n")
        .map((line, i) => [line, `${path}:${i + 1}`] as const)
        .filter(([line]) => pattern.test(line))
        .map(([line, at]) => `${at}: ${line.trim()}`),
    );
    expect(offenders).toEqual([]);
  });

  it("the menu takes its accelerators from the registry", () => {
    const menu = sources.find(([p]) => p === "app/menu.ts")?.[1] ?? "";
    expect(menu).not.toMatch(/accelerator:\s*"/);
    const used = [...menu.matchAll(/accelerator\("(\w+)"\)/g)].map((m) => m[1]);
    const withAccelerator = SHORTCUT_IDS.filter(
      (id) => "accelerator" in SHORTCUTS[id],
    );
    expect(used.sort()).toEqual([...withAccelerator].sort());
  });
});
