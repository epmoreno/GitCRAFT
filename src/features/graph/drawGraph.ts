import type { GraphLine } from "../../ipc";
import { NODE_RADIUS, ROW_HEIGHT, columnX, rowSegments } from "./geometry";

type DrawOptions = {
  lines: GraphLine[];
  scrollTop: number;
  width: number;
  height: number;
  pixelRatio: number;
  headOid: string | null;
  selectedOid: string | null;
  /** Colour behind nodes (punches the line through hollow merge nodes). */
  background: string;
  ringColor: string;
  /** Resolved lane colours (the canvas cannot use CSS variables). */
  laneColors: readonly string[];
};

/** Draws the rows visible in the viewport. Lines first, then nodes on top. */
export function drawGraph(ctx: CanvasRenderingContext2D, o: DrawOptions) {
  const laneColor = (index: number) =>
    o.laneColors[index % o.laneColors.length] ?? o.ringColor;
  ctx.setTransform(o.pixelRatio, 0, 0, o.pixelRatio, 0, 0);
  ctx.clearRect(0, 0, o.width, o.height);

  const first = Math.max(0, Math.floor(o.scrollTop / ROW_HEIGHT));
  const last = Math.min(
    o.lines.length - 1,
    Math.ceil((o.scrollTop + o.height) / ROW_HEIGHT),
  );

  ctx.lineWidth = 2;
  ctx.lineCap = "round";
  for (let i = first; i <= last; i++) {
    const line = o.lines[i];
    if (!line) continue;
    const top = i * ROW_HEIGHT - o.scrollTop;
    for (const s of rowSegments(line)) {
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
    const line = o.lines[i];
    if (!line) continue;
    const x = columnX(line.column);
    const y = i * ROW_HEIGHT - o.scrollTop + ROW_HEIGHT / 2;
    const color = laneColor(line.color);
    const isMerge = line.merge;

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

    if (line.oid === o.headOid || line.oid === o.selectedOid) {
      ctx.beginPath();
      ctx.arc(x, y, NODE_RADIUS + 3, 0, Math.PI * 2);
      ctx.strokeStyle =
        line.oid === o.headOid ? o.ringColor : laneColor(line.color);
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
  }
}
