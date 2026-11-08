import { act, render, screen, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import i18n from "../i18n";
import { SRC_DIR, srcFiles } from "../test/sources";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { useSettingsStore } from "../features/settings/store";
import en from "../i18n/en.json";
import es from "../i18n/es.json";
import {
  chordLabel,
  isShortcut,
  SHORTCUT_IDS,
  SHORTCUTS,
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
    expect(chordLabel(SHORTCUTS.changesDiscard.chords[0]!, named)).toBe("⌘⌫");
    expect(chordLabel(SHORTCUTS.previousTab.chords[0]!, named)).toBe("⌃⇧⇥");
    expect(chordLabel(SHORTCUTS.tabByNumber.chords[0]!, named)).toBe("⌘1…9");
    expect(chordLabel(SHORTCUTS.changesStage.chords[0]!, named)).toBe("< >");
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
