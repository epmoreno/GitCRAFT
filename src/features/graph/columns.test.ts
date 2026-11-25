import { DESCRIPTION_MIN, graphLayout } from "./columns";

const description = (width: number, lanes: number) => {
  const { lanes: shown, template } = graphLayout(width, lanes);
  const columns = [...template.matchAll(/(\d+)px/g)].map((m) => Number(m[1]));
  return columns.reduce((left, c) => left - c - 12, width - shown - 12);
};

describe("the history's columns", () => {
  it("shows them all when there is room", () => {
    expect(graphLayout(730, 44)).toEqual({
      lanes: 44,
      hidden: [],
      template: "minmax(0, 1fr) 150px 110px 72px",
      labels: 2,
    });
    expect(graphLayout(1200, 44).labels).toBe(3);
  });

  it("drops the hash, then the author, then the date", () => {
    expect(graphLayout(636, 44).hidden).toEqual(["sha"]);
    expect(graphLayout(420, 44).hidden).toEqual(["sha", "author"]);
    expect(graphLayout(420, 300).hidden).toEqual(["sha", "author", "date"]);
  });

  // 900 px wide, the panels leave 420 for the history.
  it.each([22, 44, 120, 320])(
    "in the narrowest window, with %d px of lanes, the summary has room",
    (lanes) => {
      expect(description(420, lanes)).toBeGreaterThanOrEqual(
        DESCRIPTION_MIN - 1,
      );
      expect(graphLayout(420, lanes).labels).toBeGreaterThanOrEqual(1);
    },
  );
});
