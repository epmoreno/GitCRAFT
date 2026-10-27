import { oid, sampleLines, sampleRefs } from "../../test/ipc";
import {
  LANE_COLORS,
  LANE_WIDTH,
  MAX_GRAPH_WIDTH,
  ROW_HEIGHT,
  columnX,
  graphWidth,
  laneColor,
  rowSegments,
} from "./geometry";
import { headOid, labelsByCommit } from "./refLabels";
import { relativeTime } from "./relativeTime";

describe("geometry", () => {
  it("places columns on a fixed grid and caps the graph width", () => {
    expect(columnX(1) - columnX(0)).toBe(LANE_WIDTH);
    expect(graphWidth(0)).toBeLessThan(graphWidth(3));
    expect(graphWidth(500)).toBe(MAX_GRAPH_WIDTH);
  });

  it("cycles the lane palette", () => {
    expect(laneColor(0)).toBe(laneColor(LANE_COLORS.length));
    expect(laneColor(1)).not.toBe(laneColor(0));
  });

  it("splits a merge row into incoming, outgoing and passing segments", () => {
    const merge = sampleLines()[1]!;
    const segments = rowSegments(merge);
    const node = { x: columnX(0), y: ROW_HEIGHT / 2 };
    expect(segments).toEqual([
      { from: { x: columnX(0), y: 0 }, to: node, color: 0 },
      { from: node, to: { x: columnX(0), y: ROW_HEIGHT }, color: 0 },
      { from: node, to: { x: columnX(1), y: ROW_HEIGHT }, color: 1 },
    ]);

    const passing = rowSegments(sampleLines()[2]!)[0];
    expect(passing).toEqual({
      from: { x: columnX(0), y: 0 },
      to: { x: columnX(0), y: ROW_HEIGHT },
      color: 0,
    });
  });
});

describe("ref labels", () => {
  it("groups labels by commit with the current branch first", () => {
    const labels = labelsByCommit(sampleRefs());
    expect(labels.get(oid("e"))).toEqual([
      { text: "main", kind: "local", current: true },
      { text: "origin/main", kind: "remote", current: false },
    ]);
    expect(labels.get(oid("b"))).toEqual([
      { text: "v1.0", kind: "tag", current: false },
    ]);
    expect(headOid(sampleRefs())).toBe(oid("e"));
  });

  it("shows a detached HEAD", () => {
    const refs = {
      ...sampleRefs(),
      head: { type: "detached" as const, oid: oid("c") },
    };
    expect(labelsByCommit(refs).get(oid("c"))?.[0]).toEqual({
      text: "HEAD",
      kind: "head",
      current: true,
    });
    expect(headOid(refs)).toBe(oid("c"));
  });

  it("handles missing refs and unborn branches", () => {
    expect(labelsByCommit(null).size).toBe(0);
    expect(
      headOid({
        head: { type: "branch", name: "main", oid: null },
        refs: [],
        remotes: [],
      }),
    ).toBeNull();
  });
});

describe("relativeTime", () => {
  const now = Date.UTC(2026, 9, 27, 12, 0, 0);
  const ago = (seconds: number) => Math.floor(now / 1000) - seconds;

  it("formats in Spanish and English", () => {
    expect(relativeTime(ago(3 * 86400), now, "es")).toBe("hace 3 días");
    expect(relativeTime(ago(3 * 86400), now, "en")).toBe("3 days ago");
    expect(relativeTime(ago(2 * 3600), now, "es")).toBe("hace 2 horas");
    expect(relativeTime(ago(90), now, "en")).toBe("1 minute ago");
    expect(relativeTime(ago(400 * 86400), now, "en")).toBe("last year");
  });

  it("says now for very recent commits", () => {
    expect(relativeTime(ago(5), now, "en")).toBe("now");
    expect(relativeTime(ago(5), now, "es")).toBe("ahora");
  });
});
