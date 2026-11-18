import type { StepAction } from "../../ipc";
import { oid } from "../../test/ipc";
import { samplePreview } from "../../test/interactiveRebase";
import {
  buildPlan,
  chainLines,
  combinedMessage,
  editCombined,
  initialRows,
  isOriginal,
  moveRow,
  rolesOf,
  withAction,
  type Row,
} from "./plan";

const p = samplePreview();
const rows = (...actions: StepAction[]): Row[] =>
  initialRows(p).map((row, i) => ({ ...row, action: actions[i] ?? "pick" }));

describe("the plan of an interactive rebase", () => {
  it("lists the newest first and keeps everything at first", () => {
    const start = initialRows(p);
    expect(start.map((r) => r.oid)).toEqual(["d", "c", "b", "a"].map(oid));
    expect(isOriginal(p, start)).toBe(true);
    const plan = buildPlan(p, start, {});
    expect(plan.steps.map((s) => s.oid)).toEqual(["a", "b", "c", "d"].map(oid));
    expect(plan).toMatchObject({ from: oid("a"), expected: "fp-1" });
  });

  it("combines with the kept commit below, through dropped ones", () => {
    // D and C into A, B dropped in between.
    const r = rows("squash", "squash", "drop", "pick");
    const roles = rolesOf(r);
    expect(roles.get(oid("a"))).toEqual({ kind: "receiver", count: 2 });
    expect(roles.get(oid("d"))).toEqual({
      kind: "combined",
      receiver: oid("a"),
    });
    expect(chainLines(r, roles)).toEqual(["start", "middle", "middle", "end"]);
    // Nothing below the oldest one.
    expect(
      rolesOf(rows("pick", "pick", "pick", "squash")).get(oid("a")),
    ).toEqual({ kind: "orphan" });
  });

  it("fills the combined message with the commits' messages, oldest first", () => {
    let r = rows("pick", "squash", "pick", "pick");
    r = withAction(p, r, oid("b"), "reword");
    r = r.map((row) =>
      row.oid === oid("b") ? { ...row, message: "B nuevo" } : row,
    );
    const roles = rolesOf(r);
    expect(combinedMessage(p, r, roles, {}, oid("b"))).toBe(
      "B nuevo\n\nwip\n\nCuerpo de c",
    );
    // Edited by hand: kept while the same commits are combined…
    const edited = editCombined(r, roles, {}, oid("b"), "Juntos");
    expect(combinedMessage(p, r, roles, edited, oid("b"))).toBe("Juntos");
    // …and filled again when another one joins.
    const more = r.map((row) =>
      row.oid === oid("d") ? { ...row, action: "squash" as const } : row,
    );
    expect(combinedMessage(p, more, rolesOf(more), edited, oid("b"))).toBe(
      "B nuevo\n\nwip\n\nCuerpo de c\n\nArregla el tooltip\n\nCuerpo de d",
    );
    // Every commit of the combination carries the message for git.
    const plan = buildPlan(p, more, edited);
    expect(plan.steps.map((s) => [s.action, s.message])).toEqual([
      ["pick", null],
      ["reword", "B nuevo"],
      [
        "squash",
        "B nuevo\n\nwip\n\nCuerpo de c\n\nArregla el tooltip\n\nCuerpo de d",
      ],
      [
        "squash",
        "B nuevo\n\nwip\n\nCuerpo de c\n\nArregla el tooltip\n\nCuerpo de d",
      ],
    ]);
  });

  it("moves rows and knows when the plan is the original again", () => {
    const start = initialRows(p);
    const moved = moveRow(start, 0, 2);
    expect(moved.map((r) => r.oid)).toEqual(["c", "b", "d", "a"].map(oid));
    expect(isOriginal(p, moved)).toBe(false);
    expect(isOriginal(p, moveRow(moved, 2, 0))).toBe(true);
    // A reword with the same message changes nothing.
    expect(isOriginal(p, withAction(p, start, oid("c"), "reword"))).toBe(true);
  });
});
