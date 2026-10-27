import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { RepoInfo } from "../../ipc";
import { drawGraph } from "./drawGraph";
import { ROW_HEIGHT, graphWidth, laneColor } from "./geometry";
import { RefChips } from "./RefChips";
import { headOid, labelsByCommit } from "./refLabels";
import { absoluteTime, relativeTime } from "./relativeTime";
import { useGraphStore } from "./store";
import styles from "./graph.module.css";

function cssVar(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return value || fallback;
}

/** Commit history of one repository: lanes on a canvas behind virtualized rows. */
export function GraphView({
  repoId,
  info,
}: {
  repoId: number;
  info: RepoInfo;
}) {
  const { t, i18n } = useTranslation();
  const graph = useGraphStore((s) => s.graphs[repoId]);
  const load = useGraphStore((s) => s.load);
  const select = useGraphStore((s) => s.select);

  useEffect(() => {
    if (!useGraphStore.getState().graphs[repoId]) void load(repoId);
  }, [repoId, load]);

  const lines = useMemo(() => graph?.lines ?? [], [graph?.lines]);
  const refs = graph?.refs ?? null;
  const selectedOid = graph?.selectedOid ?? null;
  const width = graphWidth(graph?.maxColumn ?? 0);
  const labels = useMemo(() => labelsByCommit(refs), [refs]);
  const head = useMemo(() => headOid(refs), [refs]);
  const now = Date.now();

  const scrollRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // The React Compiler is not enabled, so its memoization caveat does not apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 20,
  });

  // Bring a newly selected commit into view (e.g. picked in the sidebar).
  // Only selection changes scroll; new pages arriving must not.
  const latest = useRef({ lines, virtualizer });
  useEffect(() => {
    latest.current = { lines, virtualizer };
  });
  useEffect(() => {
    if (!selectedOid) return;
    const { lines, virtualizer } = latest.current;
    const index = lines.findIndex((l) => l.oid === selectedOid);
    if (index >= 0) virtualizer.scrollToIndex(index, { align: "auto" });
  }, [selectedOid]);

  const redraw = useCallback(() => {
    const scroller = scrollRef.current;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!scroller || !canvas || !ctx) return;
    const height = scroller.clientHeight;
    const ratio = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * ratio)) {
      canvas.width = Math.round(width * ratio);
    }
    if (canvas.height !== Math.round(height * ratio)) {
      canvas.height = Math.round(height * ratio);
    }
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    drawGraph(ctx, {
      lines,
      scrollTop: scroller.scrollTop,
      width,
      height,
      pixelRatio: ratio,
      headOid: head,
      selectedOid,
      background: cssVar("--surface-0", "#121417"),
      ringColor: cssVar("--text", "#e3e6ea"),
    });
  }, [lines, width, head, selectedOid]);

  useEffect(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    let frame = 0;
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(redraw);
    };
    schedule();
    scroller.addEventListener("scroll", schedule, { passive: true });
    const resize =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(schedule);
    resize?.observe(scroller);
    return () => {
      cancelAnimationFrame(frame);
      scroller.removeEventListener("scroll", schedule);
      resize?.disconnect();
    };
  }, [redraw]);

  const status = graph?.status ?? "loading";
  const language = i18n.language;

  return (
    <div className={styles.view}>
      <div className={styles.header} style={{ paddingLeft: width }}>
        <span>{t("graph.columnDescription")}</span>
        <span>{t("graph.columnAuthor")}</span>
        <span>{t("graph.columnDate")}</span>
        <span>{t("graph.columnCommit")}</span>
      </div>

      {graph?.error && (
        <div className={styles.loadError} role="alert">
          <p>
            {t("graph.loadFailed")}{" "}
            {t(`errors.${graph.error.kind}`, { path: info.path })}
          </p>
          {graph.error.details && (
            <pre className="selectable">{graph.error.details}</pre>
          )}
        </div>
      )}
      {status === "done" && lines.length === 0 && (
        <p className={styles.empty}>{t("graph.empty")}</p>
      )}

      <div
        ref={scrollRef}
        className={styles.scroller}
        role="listbox"
        aria-label={t("graph.listLabel", { name: info.name })}
      >
        <div className={styles.canvasLayer} aria-hidden="true">
          <canvas ref={canvasRef} className={styles.canvas} />
        </div>
        <div
          style={{ height: virtualizer.getTotalSize(), position: "relative" }}
        >
          {virtualizer.getVirtualItems().map((item) => {
            const line = lines[item.index];
            if (!line) return null;
            const selected = line.oid === selectedOid;
            return (
              <div
                key={line.oid}
                role="option"
                aria-selected={selected}
                className={
                  selected ? `${styles.row} ${styles.rowSelected}` : styles.row
                }
                style={{
                  transform: `translateY(${item.start}px)`,
                  paddingLeft: width,
                }}
                onClick={() => select(repoId, line.oid)}
              >
                <span className={styles.description}>
                  <RefChips
                    labels={labels.get(line.oid)}
                    color={laneColor(line.color)}
                  />
                  <span className={`${styles.summary} selectable`}>
                    {line.summary}
                  </span>
                </span>
                <span className={styles.author}>{line.author}</span>
                <span
                  className={styles.date}
                  title={absoluteTime(line.time, language)}
                >
                  {relativeTime(line.time, now, language)}
                </span>
                <span className={`${styles.sha} selectable`} title={line.oid}>
                  {line.oid.slice(0, 7)}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      <footer className={styles.statusbar}>
        <span className={`${styles.path} selectable`} title={info.path}>
          {info.path}
        </span>
        <span aria-live="polite">
          {status === "loading"
            ? t("graph.loadingCount", { count: lines.length })
            : t("graph.count", { count: lines.length })}
        </span>
      </footer>
    </div>
  );
}
