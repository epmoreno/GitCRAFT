/**
 * The result of a file in conflict from the user's choice for each block.
 * Pure functions: the view and its tests share them.
 *
 * Every text from the backend keeps its own line endings, so the result
 * keeps the file's (CRLF stays CRLF). Blocks without a choice are written
 * back with their markers, so saving half a file never loses a block.
 */

import type { ConflictBlock, ConflictSides, Segment } from "../../ipc";

export type Choice =
  | { kind: "ours" }
  | { kind: "theirs" }
  | { kind: "oursFirst" }
  | { kind: "theirsFirst" }
  /** Typed by the user (line endings as the textarea gives them: `\n`). */
  | { kind: "edited"; text: string };

/** Choice per block, by its index among the file's blocks. */
export type Choices = Record<number, Choice>;

/** The line ending the file uses around `block`. */
export function lineEnding(block: ConflictBlock): "\r\n" | "\n" {
  return block.oursMarker.endsWith("\r\n") ? "\r\n" : "\n";
}

/** `first` then `second`, never gluing the last line of one to the first of
 * the other. */
function join(first: string, second: string, eol: string): string {
  if (first === "" || second === "" || first.endsWith("\n")) {
    return first + second;
  }
  return first + eol + second;
}

/** Textarea text (`\n`) back in the file's line endings, ending like the
 * block it replaces so the next line is not glued to it. */
export function fromTextarea(text: string, block: ConflictBlock): string {
  const eol = lineEnding(block);
  let result = text.replace(/\r\n/g, "\n");
  const original = block.ours || block.theirs;
  if (result !== "" && !result.endsWith("\n") && original.endsWith("\n")) {
    result += "\n";
  }
  return eol === "\r\n" ? result.replace(/\n/g, "\r\n") : result;
}

/** The same text as the textarea shows it. */
export function toTextarea(text: string): string {
  return text.replace(/\r\n/g, "\n");
}

/** The text a choice puts in place of `block`. */
export function chosenText(block: ConflictBlock, choice: Choice): string {
  const eol = lineEnding(block);
  switch (choice.kind) {
    case "ours":
      return block.ours;
    case "theirs":
      return block.theirs;
    case "oursFirst":
      return join(block.ours, block.theirs, eol);
    case "theirsFirst":
      return join(block.theirs, block.ours, eol);
    case "edited":
      return fromTextarea(choice.text, block);
  }
}

/** The block exactly as it is in the file, markers included. */
export function rawBlock(block: ConflictBlock): string {
  return (
    block.oursMarker +
    block.ours +
    (block.baseMarker !== null ? block.baseMarker + (block.base ?? "") : "") +
    block.separator +
    block.theirs +
    block.theirsMarker
  );
}

/** For each segment, its index among the blocks (`null` for common text). */
export function blockIndexes(segments: Segment[]): (number | null)[] {
  let next = 0;
  return segments.map((s) => (s.type === "conflict" ? next++ : null));
}

export function blocksOf(segments: Segment[]): ConflictBlock[] {
  return segments.flatMap((s) => (s.type === "conflict" ? [s.block] : []));
}

/** The whole file with the choices applied. */
export function buildResult(segments: Segment[], choices: Choices): string {
  let index = 0;
  let out = "";
  for (const segment of segments) {
    if (segment.type === "common") {
      out += segment.text;
      continue;
    }
    const choice = choices[index];
    out += choice ? chosenText(segment.block, choice) : rawBlock(segment.block);
    index += 1;
  }
  return out;
}

/** Number of lines of `text` (a final line without ending counts). */
export function lineCount(text: string): number {
  if (text === "") return 0;
  const breaks = text.match(/\n/g)?.length ?? 0;
  return text.endsWith("\n") ? breaks : breaks + 1;
}

/** How each side of the conflict is called in buttons and headers. */
export type SideNames = {
  ours: string;
  theirs: string;
};

/** The names of both sides: branches when known, else short commit ids.
 * In a rebase the left side is the new base and the right one your
 * branch's commit being replayed. */
export function sideNames(
  sides: ConflictSides | null,
  revertLabel: (short: string) => string,
): SideNames {
  if (!sides) return { ours: "HEAD", theirs: "?" };
  switch (sides.type) {
    case "merge":
      return {
        ours: sides.branch ?? sides.head.short,
        theirs: sides.merging.branches[0] ?? sides.merging.commit.short,
      };
    case "rebase":
      return {
        ours: sides.onto.branches[0] ?? sides.onto.commit.short,
        theirs: sides.branch ?? sides.applying?.short ?? "?",
      };
    case "cherryPick":
      return {
        ours: sides.branch ?? sides.head.short,
        theirs: sides.picking.short,
      };
    case "revert":
      return {
        ours: sides.branch ?? sides.head.short,
        theirs: revertLabel(sides.reverting.short),
      };
  }
}

/** Names longer than this are cut with "…" (the full name goes in a
 * tooltip). */
export const NAME_MAX = 24;

export function shorten(name: string, max = NAME_MAX): string {
  const chars = [...name];
  return chars.length <= max ? name : `${chars.slice(0, max - 1).join("")}…`;
}
