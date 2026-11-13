import type { ConflictBlock } from "../ipc";

/** A conflict block as the backend sends it (markers with the line ending
 * the sides use). */
export function block(
  ours: string,
  theirs: string,
  overrides: Partial<ConflictBlock> = {},
): ConflictBlock {
  const eol = ours.includes("\r\n") || theirs.includes("\r\n") ? "\r\n" : "\n";
  return {
    ours,
    theirs,
    base: null,
    oursMarker: `<<<<<<< HEAD${eol}`,
    baseMarker: null,
    separator: `=======${eol}`,
    theirsMarker: `>>>>>>> feature${eol}`,
    ...overrides,
  };
}
