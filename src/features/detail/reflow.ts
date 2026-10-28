/**
 * Re-flows a commit message body for a panel narrower or wider than the
 * ~72 columns git messages are wrapped at: the lines of one paragraph are
 * joined so the panel can wrap them itself.
 *
 * Kept as they are:
 * - paragraphs (separated by blank lines);
 * - list items ("- ", "* ", "+ ", "1. ", "1) "): each starts a new line, and
 *   its wrapped continuation lines are joined into it;
 * - indented blocks (a tab or 4+ spaces, outside a list item): code, logs;
 * - trailer paragraphs ("Signed-off-by: …", "Co-authored-by: …").
 */
export function reflowMessage(body: string): string {
  const paragraphs = body.replace(/\r\n/g, "\n").split(/\n[ \t]*\n/);
  return paragraphs
    .map((p) => p.replace(/^\n+|\n+$/g, ""))
    .filter((p) => p.trim() !== "")
    .map(reflowParagraph)
    .join("\n\n");
}

const LIST_ITEM = /^\s*(?:[-*+]|\d+[.)])\s+\S/;
const CODE = /^(?:\t| {4,})/;
const TRAILER = /^[A-Za-z][A-Za-z0-9-]*: \S/;

type Block = { kind: "prose" | "item" | "code"; text: string };

function reflowParagraph(paragraph: string): string {
  const lines = paragraph.split("\n");
  if (lines.every((line) => TRAILER.test(line))) return paragraph;

  const blocks: Block[] = [];
  for (const line of lines) {
    const last = blocks.at(-1);
    if (LIST_ITEM.test(line)) {
      blocks.push({ kind: "item", text: line.trimEnd() });
    } else if (
      last?.kind === "item" &&
      line.trim() !== "" &&
      !CODE.test(line)
    ) {
      last.text += ` ${line.trim()}`;
    } else if (
      last?.kind === "item" &&
      CODE.test(line) &&
      /^\s{1,7}\S/.test(line)
    ) {
      // Continuation indented under its item (2–7 spaces is still the item).
      last.text += ` ${line.trim()}`;
    } else if (CODE.test(line)) {
      blocks.push({ kind: "code", text: line.trimEnd() });
    } else if (last?.kind === "prose") {
      last.text += ` ${line.trim()}`;
    } else {
      blocks.push({ kind: "prose", text: line.trim() });
    }
  }
  return blocks.map((b) => b.text).join("\n");
}
