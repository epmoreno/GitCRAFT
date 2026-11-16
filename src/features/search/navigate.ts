import type { GraphLine } from "../../ipc";

/**
 * Moving between search results in the graph's order (task 12.2): "next"
 * goes down, towards older commits.
 */

/** Index in `lines` of the first match after (or before) `from`, or -1. */
export function nextMatch(
  lines: readonly GraphLine[],
  matches: ReadonlySet<string>,
  from: number,
  direction: "next" | "previous",
): number {
  if (direction === "next") {
    for (let i = from + 1; i < lines.length; i++) {
      if (matches.has(lines[i]!.oid)) return i;
    }
  } else {
    for (let i = Math.min(from, lines.length) - 1; i >= 0; i--) {
      if (matches.has(lines[i]!.oid)) return i;
    }
  }
  return -1;
}

/** Results the graph has already loaded. */
export function loadedMatches(
  lines: readonly GraphLine[],
  matches: ReadonlySet<string>,
): number {
  let count = 0;
  for (const line of lines) if (matches.has(line.oid)) count++;
  return count;
}

/** 1-based position of `oid` among the loaded results, or 0. */
export function positionOf(
  lines: readonly GraphLine[],
  matches: ReadonlySet<string>,
  oid: string | null,
): number {
  if (!oid || !matches.has(oid)) return 0;
  let position = 0;
  for (const line of lines) {
    if (matches.has(line.oid)) position++;
    if (line.oid === oid) return position;
  }
  return 0;
}
