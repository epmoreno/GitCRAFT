import type { GraphEntry } from "../../ipc";
import {
  NODE_RADIUS,
  ROW_HEIGHT,
  columnX,
  laneColor,
  rowSegments,
} from "./geometry";

type DrawOptions = {
  entries: GraphEntry[];
  scrollTop: number;
  width: number;
  height: number;
  pixelRatio: number;
  headOid: string | null;
  selectedOid: string | null;
  /** Colour behind nodes (punches the line through hollow merge nodes). */
  background: string;
  ringColor: string;
};

/** Draws the rows visible in the viewport. Lines first, then nodes on top. */
export function drawGraph(ctx: CanvasRenderingContext2D, o: DrawOptions) {
  ctx.setTransform(o.pixelRatio, 0, 0, o.pixelRatio, 0, 0);
  ctx.clearRect(0, 0, o.width, o.height);

  const first = Math.max(0, Math.floor(o.scrollTop / ROW_HEIGHT));
  const last = Math.min(
    o.entries.length - 1,
    Math.ceil((o.scrollTop + o.height) / ROW_HEIGHT),
  );

  ctx.lineWidth = 2;
  ctx.lineCap = "round";
  for (let i = first; i <= last; i++) {
    const entry = o.entries[i];
    if (!entry) continue;
    const top = i * ROW_HEIGHT - o.scrollTop;
    for (const s of rowSegments(entry.row)) {
      ctx.strokeStyle = laneColor(s.color);
      ctx.beginPath();
      ctx.moveTo(s.from.x, top + s.from.y);
      if (s.from.x === s.to.x) {
        ctx.lineTo(s.to.x, top + s.to.y);
      } else {
        const midY = top + (s.from.y + s.to.y) / 2;
        ctx.bezierCurveTo(s.from.x, midY, s.to.x, midY, s.to.x, top + s.to.y);
      }
      ctx.stroke();
    }
  }

  for (let i = first; i <= last; i++) {
    const entry = o.entries[i];
    if (!entry) continue;
    const x = columnX(entry.row.column);
    const y = i * ROW_HEIGHT - o.scrollTop + ROW_HEIGHT / 2;
    const color = laneColor(entry.row.color);
    const isMerge = entry.commit.parents.length > 1;

    ctx.beginPath();
    ctx.arc(x, y, NODE_RADIUS, 0, Math.PI * 2);
    if (isMerge) {
      ctx.fillStyle = o.background;
      ctx.fill();
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.stroke();
    } else {
      ctx.fillStyle = color;
      ctx.fill();
    }

    if (entry.commit.oid === o.headOid || entry.commit.oid === o.selectedOid) {
      ctx.beginPath();
      ctx.arc(x, y, NODE_RADIUS + 3, 0, Math.PI * 2);
      ctx.strokeStyle =
        entry.commit.oid === o.headOid
          ? o.ringColor
          : laneColor(entry.row.color);
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
  }
}
