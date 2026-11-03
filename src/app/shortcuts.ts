/**
 * Every keyboard shortcut of the app, in one place: the key handlers and the
 * menu bar read them from here, and Settings → Shortcuts lists them, so the
 * list cannot drift from what the keys do (a test checks that no handler
 * compares keys on its own).
 */

/** A key and its modifiers (missing modifiers must not be pressed). */
type Chord = {
  /** `KeyboardEvent.key`; `"1-9"` stands for any digit 1 to 9 and
   * `"click"` for a mouse click. */
  key: string;
  meta?: boolean;
  ctrl?: boolean;
  shift?: boolean;
};

export type ShortcutGroup =
  "app" | "tabs" | "changes" | "commit" | "diff" | "general";

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
    chords: [{ key: ",", meta: true }],
    accelerator: "CmdOrCtrl+,",
  },
  newTab: {
    group: "tabs",
    chords: [{ key: "t", meta: true }],
    accelerator: "CmdOrCtrl+T",
  },
  closeTab: {
    group: "tabs",
    chords: [{ key: "w", meta: true }],
    accelerator: "CmdOrCtrl+W",
  },
  tabByNumber: { group: "tabs", chords: [{ key: "1-9", meta: true }] },
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
    chords: [{ key: "Backspace", meta: true }],
  },
  commit: {
    group: "commit",
    chords: [
      { key: "Enter", meta: true },
      { key: "Enter", ctrl: true },
    ],
  },
  commitBoxGrow: { group: "commit", chords: [{ key: "ArrowUp" }] },
  commitBoxShrink: { group: "commit", chords: [{ key: "ArrowDown" }] },
  commitBoxLargest: { group: "commit", chords: [{ key: "Home" }] },
  commitBoxSmallest: { group: "commit", chords: [{ key: "End" }] },
  commitBoxReset: { group: "commit", chords: [{ key: "Enter" }] },
  diffRange: { group: "diff", chords: [{ key: "click", shift: true }] },
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
    !event.altKey
  );
}

/** Whether `event` is shortcut `id`. */
export function isShortcut(id: ShortcutId, event: KeyLike): boolean {
  return SHORTCUTS[id].chords.some((chord) => chordMatches(chord, event));
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

const KEY_SYMBOLS: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Enter: "↩",
  Backspace: "⌫",
  Tab: "⇥",
};

/** Keys written with their name (translated by `named`), not a symbol. */
const NAMED_KEYS = new Set([" ", "Home", "End", "Escape", "click"]);

/** How a chord is written on a Mac ("⌘⌫", "⌃⇧⇥"); `named` translates keys
 * that have a name rather than a symbol (Space, Home, End, Esc, click). */
export function chordLabel(chord: Chord, named: (key: string) => string) {
  const key =
    chord.key === "1-9"
      ? "1…9"
      : NAMED_KEYS.has(chord.key)
        ? named(chord.key)
        : (KEY_SYMBOLS[chord.key] ?? chord.key.toUpperCase());
  return `${chord.ctrl ? "⌃" : ""}${chord.shift ? "⇧" : ""}${chord.meta ? "⌘" : ""}${key}`;
}
