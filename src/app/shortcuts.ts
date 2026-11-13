/**
 * Every keyboard shortcut of the app, in one place: the key handlers and the
 * menu bar read them from here, and Settings → Shortcuts lists them, so the
 * list cannot drift from what the keys do (a test checks that no handler
 * compares keys on its own).
 *
 * `primary` is the main modifier of each system: ⌘ on macOS, Ctrl on Windows
 * and Linux (the menu's `CmdOrCtrl`). A chord with `os` exists only there.
 */

import { platform, type Platform } from "./platform";

/** A key and its modifiers (missing modifiers must not be pressed). */
type Chord = {
  /** `KeyboardEvent.key`; `"1-9"` stands for any digit 1 to 9 and
   * `"click"` for a mouse click. */
  key: string;
  /** ⌘ on macOS, Ctrl elsewhere. */
  primary?: boolean;
  meta?: boolean;
  ctrl?: boolean;
  shift?: boolean;
  /** ⌥ on macOS, Alt elsewhere. */
  alt?: boolean;
  /** Only on this system. */
  os?: Platform;
};

export type ShortcutGroup =
  "app" | "tabs" | "changes" | "commit" | "diff" | "conflicts" | "general";

export type Shortcut = {
  group: ShortcutGroup;
  /** Any of these chords triggers it. */
  chords: Chord[];
  /** Same shortcut in the native menu bar (Tauri accelerator syntax). */
  accelerator?: string;
};

export const SHORTCUTS = {
  settings: {
    group: "app",
    chords: [{ key: ",", primary: true }],
    accelerator: "CmdOrCtrl+,",
  },
  newTab: {
    group: "tabs",
    chords: [{ key: "t", primary: true }],
    accelerator: "CmdOrCtrl+T",
  },
  closeTab: {
    group: "tabs",
    chords: [{ key: "w", primary: true }],
    accelerator: "CmdOrCtrl+W",
  },
  tabByNumber: { group: "tabs", chords: [{ key: "1-9", primary: true }] },
  nextTab: { group: "tabs", chords: [{ key: "Tab", ctrl: true }] },
  previousTab: {
    group: "tabs",
    chords: [{ key: "Tab", ctrl: true, shift: true }],
  },
  changesNext: { group: "changes", chords: [{ key: "ArrowDown" }] },
  changesPrevious: { group: "changes", chords: [{ key: "ArrowUp" }] },
  changesFirst: { group: "changes", chords: [{ key: "Home" }] },
  changesLast: { group: "changes", chords: [{ key: "End" }] },
  changesExpand: { group: "changes", chords: [{ key: "ArrowRight" }] },
  changesCollapse: { group: "changes", chords: [{ key: "ArrowLeft" }] },
  changesOpen: { group: "changes", chords: [{ key: "Enter" }] },
  changesStage: { group: "changes", chords: [{ key: " " }] },
  changesDiscard: {
    group: "changes",
    chords: [
      { key: "Backspace", primary: true, os: "mac" },
      { key: "Delete", primary: true, os: "other" },
    ],
  },
  commit: {
    group: "commit",
    chords: [
      { key: "Enter", primary: true },
      { key: "Enter", ctrl: true, os: "mac" },
    ],
  },
  commitBoxGrow: { group: "commit", chords: [{ key: "ArrowUp" }] },
  commitBoxShrink: { group: "commit", chords: [{ key: "ArrowDown" }] },
  commitBoxLargest: { group: "commit", chords: [{ key: "Home" }] },
  commitBoxSmallest: { group: "commit", chords: [{ key: "End" }] },
  commitBoxReset: { group: "commit", chords: [{ key: "Enter" }] },
  diffRange: { group: "diff", chords: [{ key: "click", shift: true }] },
  conflictPrevious: {
    group: "conflicts",
    chords: [{ key: "ArrowUp", alt: true }],
  },
  conflictNext: {
    group: "conflicts",
    chords: [{ key: "ArrowDown", alt: true }],
  },
  escape: { group: "general", chords: [{ key: "Escape" }] },
  menuNext: { group: "general", chords: [{ key: "ArrowDown" }] },
  menuPrevious: { group: "general", chords: [{ key: "ArrowUp" }] },
} satisfies Record<string, Shortcut>;

export type ShortcutId = keyof typeof SHORTCUTS;

export const SHORTCUT_IDS = Object.keys(SHORTCUTS) as ShortcutId[];

type KeyLike = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey"
>;

/** The chords of `id` on this system, with `primary` made concrete. */
export function chordsOf(id: ShortcutId): Chord[] {
  const os = platform();
  return (SHORTCUTS[id].chords as Chord[])
    .filter((chord) => chord.os === undefined || chord.os === os)
    .map((chord) => ({
      key: chord.key,
      meta: chord.meta || (chord.primary && os === "mac"),
      ctrl: chord.ctrl || (chord.primary && os === "other"),
      shift: chord.shift,
      alt: chord.alt,
    }));
}

function chordMatches(chord: Chord, event: KeyLike): boolean {
  const key =
    chord.key === "1-9"
      ? /^[1-9]$/.test(event.key)
      : event.key.toLowerCase() === chord.key.toLowerCase();
  return (
    key &&
    !!chord.meta === event.metaKey &&
    !!chord.ctrl === event.ctrlKey &&
    !!chord.shift === event.shiftKey &&
    !!chord.alt === event.altKey
  );
}

/** Whether `event` is shortcut `id`. */
export function isShortcut(id: ShortcutId, event: KeyLike): boolean {
  return chordsOf(id).some((chord) => chordMatches(chord, event));
}

/** The first of `ids` that `event` is, if any. */
export function whichShortcut<T extends ShortcutId>(
  ids: readonly T[],
  event: KeyLike,
): T | null {
  return ids.find((id) => isShortcut(id, event)) ?? null;
}

/** ⇧-click on a diff line: select the range from the previous one. */
export function isRangeClick(event: Pick<MouseEvent, "shiftKey">): boolean {
  return event.shiftKey;
}

/** The accelerator of a menu shortcut. */
export function accelerator(id: ShortcutId): string {
  const value = (SHORTCUTS[id] as Shortcut).accelerator;
  if (!value) throw new Error(`shortcut ${id} has no accelerator`);
  return value;
}

const MAC_SYMBOLS: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↩",
  Backspace: "⌫",
  Tab: "⇥",
};

const ARROWS: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/** Keys written with their name (translated by `named`), not a symbol. */
const NAMED_KEYS = new Set([" ", "Home", "End", "Escape", "click"]);

/** Also named outside macOS, as Windows and Linux keyboards print them. */
const NAMED_KEYS_OTHER = new Set([
  ...NAMED_KEYS,
  "Enter",
  "Backspace",
  "Delete",
  "Tab",
  "Shift",
]);

/**
 * How a concrete chord (from `chordsOf`) is written on this system: "⌘⌫",
 * "⌃⇧⇥" on macOS; "Ctrl+Supr", "Ctrl+Mayús+Tab" elsewhere. `named`
 * translates keys that have a name rather than a symbol.
 */
export function chordLabel(chord: Chord, named: (key: string) => string) {
  const digits = chord.key === "1-9" ? "1…9" : null;
  if (platform() === "mac") {
    const key =
      digits ??
      (NAMED_KEYS.has(chord.key)
        ? named(chord.key)
        : (MAC_SYMBOLS[chord.key] ?? chord.key.toUpperCase()));
    return `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}${chord.shift ? "⇧" : ""}${chord.meta ? "⌘" : ""}${key}`;
  }
  const key =
    digits ??
    (NAMED_KEYS_OTHER.has(chord.key)
      ? named(chord.key)
      : (ARROWS[chord.key] ?? chord.key.toUpperCase()));
  return [
    chord.ctrl && "Ctrl",
    chord.alt && "Alt",
    chord.shift && named("Shift"),
    chord.meta && "Meta",
    key,
  ]
    .filter(Boolean)
    .join("+");
}

/** The label of the first chord of `id` on this system, for hints such as
 * "Close tab (⌘W)"; `key` replaces the chord's key (a tab's number). */
export function shortcutLabel(
  id: ShortcutId,
  named: (key: string) => string,
  key?: string,
): string {
  const [chord] = chordsOf(id);
  if (!chord) throw new Error(`shortcut ${id} has no chord here`);
  return chordLabel(key === undefined ? chord : { ...chord, key }, named);
}
