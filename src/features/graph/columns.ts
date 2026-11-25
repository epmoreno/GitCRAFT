/** The columns after the description, and what they take. The first to go
 * when the history is narrow is the last one. */
const WIDTHS = { author: 150, date: 110, sha: 72 } as const;
const GAP = 12;
export type GraphColumn = keyof typeof WIDTHS;

/** The order they go in: the detail panel always has the hash and the author
 * of the selected commit, and each row's tooltip has them all. */
const GOING: GraphColumn[] = ["sha", "author", "date"];

/** What the description (labels and summary) keeps before a column goes. */
export const DESCRIPTION_MIN = 220;
/** The lanes take at most this part of the history. */
const LANES_MAX = 0.45;
/** Room for one label: shorter ones take less. */
const LABEL = 66;
const MAX_LABELS = 3;

export type GraphLayout = {
  lanes: number;
  hidden: GraphColumn[];
  /** For the `grid-template-columns` of the header and the rows. */
  template: string;
  /** Labels shown before "+N". */
  labels: number;
};

/** What the history shows in `width` px, with `lanes` px of lanes. */
export function graphLayout(width: number, lanes: number): GraphLayout {
  const shownLanes = Math.min(lanes, Math.round(width * LANES_MAX));
  const all = Object.keys(WIDTHS) as GraphColumn[];
  const description = (hidden: GraphColumn[]) =>
    all
      .filter((c) => !hidden.includes(c))
      .reduce((left, c) => left - WIDTHS[c] - GAP, width - shownLanes - GAP);
  let hidden: GraphColumn[] = [];
  for (const column of GOING) {
    if (description(hidden) >= DESCRIPTION_MIN) break;
    hidden = [...hidden, column];
  }
  // Labels take at most 60 % of it, with room for "+N".
  const labels = Math.floor((description(hidden) * 0.6 - 30) / LABEL);
  return {
    lanes: shownLanes,
    hidden,
    template: [
      "minmax(0, 1fr)",
      ...all.filter((c) => !hidden.includes(c)).map((c) => `${WIDTHS[c]}px`),
    ].join(" "),
    labels: Math.max(1, Math.min(MAX_LABELS, labels)),
  };
}
