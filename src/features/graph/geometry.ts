import type { GraphLine } from "../../ipc";

export const ROW_HEIGHT = 26;
export const LANE_WIDTH = 14;
export const GRAPH_PADDING = 8;
export const MAX_GRAPH_WIDTH = 320;
export const NODE_RADIUS = 4.5;

/** Number of lane colours (`--lane-1` … `--lane-8` in tokens.css). */
export const LANE_COUNT = 8;

/** Lane colour as a CSS value, for DOM elements (chips). */
export function laneColor(index: number): string {
  return `var(--lane-${(index % LANE_COUNT) + 1})`;
}

export function columnX(column: number): number {
  return GRAPH_PADDING + column * LANE_WIDTH + LANE_WIDTH / 2;
}

export function graphWidth(maxColumn: number): number {
  return Math.min(
    MAX_GRAPH_WIDTH,
    GRAPH_PADDING * 2 + (maxColumn + 1) * LANE_WIDTH,
  );
}

export type Point = { x: number; y: number };

/** A straight line or an S-curve (cubic Bézier with vertical tangents). */
export type Segment = { from: Point; to: Point; color: number };

/** Line segments of one row, in pixels relative to the row's top edge. */
export function rowSegments(row: GraphLine): Segment[] {
  const top = 0;
  const middle = ROW_HEIGHT / 2;
  const bottom = ROW_HEIGHT;
  const node = columnX(row.column);
  const segments: Segment[] = [];

  for (const [column, color] of row.passing) {
    const x = columnX(column);
    segments.push({ from: { x, y: top }, to: { x, y: bottom }, color });
  }
  for (const [column, color] of row.incoming) {
    segments.push({
      from: { x: columnX(column), y: top },
      to: { x: node, y: middle },
      color,
    });
  }
  for (const [column, color] of row.outgoing) {
    segments.push({
      from: { x: node, y: middle },
      to: { x: columnX(column), y: bottom },
      color,
    });
  }
  return segments;
}
