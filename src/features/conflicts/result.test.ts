import type { ConflictSides, Segment } from "../../ipc";
import { block } from "../../test/conflicts";
import {
  buildResult,
  chosenText,
  fromTextarea,
  shorten,
  sideNames,
  toTextarea,
} from "./result";

const commit = (short: string, subject = "subject") => ({
  oid: short.padEnd(40, "0"),
  short,
  subject,
});

describe("the result of a file in conflict", () => {
  const b = block("main\n", "feature\n");

  it("puts each choice in place of its block", () => {
    expect(chosenText(b, { kind: "ours" })).toBe("main\n");
    expect(chosenText(b, { kind: "theirs" })).toBe("feature\n");
    expect(chosenText(b, { kind: "oursFirst" })).toBe("main\nfeature\n");
    expect(chosenText(b, { kind: "theirsFirst" })).toBe("feature\nmain\n");
  });

  it("never glues two lines when a side has no final line ending", () => {
    const last = block("main", "feature");
    expect(chosenText(last, { kind: "oursFirst" })).toBe("main\nfeature");
    const crlf = block("main", "feature", { oursMarker: "<<<<<<< HEAD\r\n" });
    expect(chosenText(crlf, { kind: "theirsFirst" })).toBe("feature\r\nmain");
  });

  it("keeps CRLF, also in text typed in the textarea", () => {
    const crlf = block("a\r\nb\r\n", "c\r\n");
    expect(toTextarea(crlf.ours)).toBe("a\nb\n");
    expect(chosenText(crlf, { kind: "edited", text: "x\ny" })).toBe(
      "x\r\ny\r\n",
    );
    expect(fromTextarea("", crlf)).toBe("");
  });

  it("writes blocks without a choice back with their markers", () => {
    const segments: Segment[] = [
      { type: "common", text: "top\n" },
      { type: "conflict", block: b },
      { type: "common", text: "middle\n" },
      {
        type: "conflict",
        block: block("x\n", "y\n", {
          baseMarker: "||||||| base\n",
          base: "o\n",
        }),
      },
    ];
    expect(buildResult(segments, { 1: { kind: "theirs" } })).toBe(
      "top\n<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature\nmiddle\ny\n",
    );
    expect(buildResult(segments, {})).toContain(
      "<<<<<<< HEAD\nx\n||||||| base\no\n=======\ny\n",
    );
  });

  it("leaves out a base that is not in the file", () => {
    const computed = block("a\n", "b\n", { base: "o\n" });
    expect(buildResult([{ type: "conflict", block: computed }], {})).toBe(
      "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> feature\n",
    );
  });
});

describe("names of the sides", () => {
  const revert = (short: string) => `undo ${short}`;

  it("uses branches when known, short ids otherwise", () => {
    const merge: ConflictSides = {
      type: "merge",
      branch: "main",
      head: commit("aaaaaaa"),
      merging: { commit: commit("bbbbbbb"), branches: ["feature"] },
    };
    expect(sideNames(merge, revert)).toEqual({
      ours: "main",
      theirs: "feature",
    });
    expect(
      sideNames(
        { ...merge, merging: { commit: commit("bbbbbbb"), branches: [] } },
        revert,
      ).theirs,
    ).toBe("bbbbbbb");
  });

  it("in a rebase, the new base on the left and your branch on the right", () => {
    const rebase: ConflictSides = {
      type: "rebase",
      branch: "topic",
      onto: { commit: commit("ccccccc"), branches: ["main"] },
      head: commit("ccccccc"),
      applying: commit("ddddddd", "topic one"),
      progress: { current: 1, total: 3 },
    };
    expect(sideNames(rebase, revert)).toEqual({
      ours: "main",
      theirs: "topic",
    });
  });

  it("names a revert by what it undoes", () => {
    const sides: ConflictSides = {
      type: "revert",
      branch: "main",
      head: commit("aaaaaaa"),
      reverting: commit("eeeeeee"),
    };
    expect(sideNames(sides, revert).theirs).toBe("undo eeeeeee");
  });

  it("cuts long names with an ellipsis", () => {
    expect(shorten("feature/short")).toBe("feature/short");
    const long = "feature/a-very-long-branch-name-indeed";
    expect(shorten(long)).toHaveLength(24);
    expect(shorten(long).endsWith("…")).toBe(true);
  });
});
