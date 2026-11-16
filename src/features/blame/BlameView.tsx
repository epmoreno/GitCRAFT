import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isShortcut, whichShortcut } from "../../app/shortcuts";
import { useShortcutLabel } from "../../app/useShortcutLabel";
import {
  blameFile,
  commands,
  type BlameChunk,
  type BlameCommit,
  type BlameOutcome,
  type CommandError,
} from "../../ipc";
import { ContextMenu } from "../../ui/ContextMenu";
import { ErrorReport } from "../../ui/ErrorReport";
import { WidthHandle } from "../../ui/WidthHandle";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { useGraphStore } from "../graph/store";
import styles from "./blame.module.css";

export const LINE_HEIGHT = 20;

/** What has arrived so far. `owner` maps each line (0-based) to its group
 * in `chunks`; it and the lists grow in place, `version` says when. */
type Blamed = {
  lines: string[] | null;
  chunks: BlameChunk[];
  commits: Map<string, BlameCommit>;
  owner: (number | undefined)[];
  attributed: number;
  version: number;
  outcome: BlameOutcome | null;
  error: CommandError | null;
  running: boolean;
};

const empty = (): Blamed => ({
  lines: null,
  chunks: [],
  commits: new Map(),
  owner: [],
  attributed: 0,
  version: 0,
  outcome: null,
  error: null,
  running: true,
});

/**
 * The blame of a file (task 12.6), over the graph: each line with the
 * commit it comes from, arriving as git attributes it. "Blame before this
 * commit" keeps a way back ("← Previous blame", ⌘[ / Alt+←).
 */
export function BlameView({
  repoId,
  path,
  rev,
  chosen,
  canGoBack,
}: {
  repoId: number;
  path: string;
  rev: string;
  chosen: string | null;
  canGoBack: boolean;
}) {
  const { t, i18n } = useTranslation();
  const label = useShortcutLabel();
  const close = useGraphStore((s) => s.closeFile);
  const choose = useGraphStore((s) => s.chooseInHistory);
  const select = useGraphStore((s) => s.select);
  const before = useGraphStore((s) => s.blameBefore);
  const back = useGraphStore((s) => s.blameBack);
  const open = useGraphStore((s) => s.openFileDiff);
  const [force, setForce] = useState(false);
  const [blamed, setBlamed] = useState<Blamed>(empty);
  const [width, setWidth] = useState<number | null>(null);
  const [menu, setMenu] = useState<{
    chunk: BlameChunk;
    x: number;
    y: number;
  } | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<HTMLElement>(null);
  const now = Date.now();

  useEffect(() => {
    let stale = false;
    const state = empty();
    setBlamed({ ...state });
    void blameFile(repoId, path, rev, force, (event) => {
      if (stale) return;
      if (event.type === "text") {
        state.lines = event.lines;
        state.owner = new Array(event.lines.length);
      } else {
        for (const commit of event.commits) {
          state.commits.set(commit.oid, commit);
        }
        for (const chunk of event.chunks) {
          const index = state.chunks.push(chunk) - 1;
          for (let i = 0; i < chunk.count; i++) {
            state.owner[chunk.line - 1 + i] = index;
          }
        }
        state.attributed = event.attributed;
      }
      state.version++;
      setBlamed({ ...state });
    }).then((result) => {
      if (stale) return;
      state.running = false;
      if (result.status === "ok") state.outcome = result.data;
      else if (result.error.kind !== "cancelled") state.error = result.error;
      setBlamed({ ...state });
    });
    return () => {
      stale = true;
      void commands.cancelBlame(repoId);
    };
  }, [repoId, path, rev, force]);

  const { lines, chunks, commits, owner, attributed, outcome, running } =
    blamed;
  const total = lines?.length ?? 0;

  // Alternate bands in the order the groups appear in the file.
  const band = useMemo(() => {
    const order = chunks
      .map((chunk, index) => ({ index, line: chunk.line }))
      .sort((a, b) => a.line - b.line);
    const bands = new Map<number, number>();
    order.forEach(({ index }, position) => bands.set(index, position % 2));
    return bands;
    // `chunks` grows in place: `version` says when.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [chunks, blamed.version]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (document.querySelector('[role="menu"], [aria-modal="true"]')) return;
      if (isShortcut("escape", event)) close(repoId);
      else if (canGoBack && isShortcut("blameBack", event)) {
        event.preventDefault();
        back(repoId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId, close, back, canGoBack]);

  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => LINE_HEIGHT,
    overscan: 30,
  });

  /** The first line (0-based) of the group before or after `line`. */
  const neighbour = (line: number, direction: "next" | "previous") => {
    const current = owner[line];
    const step = direction === "next" ? 1 : -1;
    let i = line + step;
    while (i >= 0 && i < total && owner[i] === current) i += step;
    if (i < 0 || i >= total) return -1;
    const target = owner[i];
    if (target === undefined) return i;
    return (chunks[target]?.line ?? i + 1) - 1;
  };

  const focusLine = (line: number) => {
    virtualizer.scrollToIndex(line, { align: "auto" });
    requestAnimationFrame(() =>
      scrollRef.current
        ?.querySelector<HTMLElement>(`[data-chunk-line="${line}"]`)
        ?.focus(),
    );
  };

  const openMenu = (chunk: BlameChunk, element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setMenu({ chunk, x: rect.left + 24, y: rect.bottom });
  };

  const short = rev === "HEAD" ? "HEAD" : rev.slice(0, 7);
  const percent = total > 0 ? Math.round((attributed / total) * 100) : 0;

  const notice = (() => {
    if (blamed.error) {
      return <ErrorReport error={blamed.error} lead={t("blame.failed")} />;
    }
    switch (outcome?.type) {
      case "binary":
        return <p className={styles.notice}>{t("blame.binary")}</p>;
      case "missing":
        return <p className={styles.notice}>{t("blame.missing")}</p>;
      case "tooLarge":
        return (
          <p className={styles.notice}>
            {t("blame.tooLarge", {
              size: Math.ceil((outcome.bytes ?? 0) / (1024 * 1024)),
            })}
          </p>
        );
      case "large":
        return (
          <div className={styles.notice} role="alert">
            <p>
              {t("blame.large", {
                count: outcome.lines,
                lines: outcome.lines.toLocaleString(i18n.language),
              })}
            </p>
            <div className={styles.actions}>
              <button
                type="button"
                className={styles.primary}
                onClick={() => setForce(true)}
              >
                {t("blame.start")}
              </button>
              <button
                type="button"
                className={styles.button}
                onClick={() => (canGoBack ? back(repoId) : close(repoId))}
              >
                {t("blame.cancelLarge")}
              </button>
            </div>
          </div>
        );
      default:
        return null;
    }
  })();

  return (
    <section
      ref={viewRef}
      className={styles.view}
      aria-label={t("blame.label", { path })}
      style={
        width === null
          ? undefined
          : ({ "--blame-gutter": `${width}px` } as React.CSSProperties)
      }
    >
      <header className={styles.header}>
        <button
          type="button"
          className={styles.button}
          title={t("blame.backToGraphHint")}
          onClick={() => close(repoId)}
        >
          ← {t("blame.backToGraph")}
        </button>
        {canGoBack && (
          <button
            type="button"
            className={styles.button}
            title={t("blame.previousHint", { shortcut: label("blameBack") })}
            onClick={() => back(repoId)}
          >
            ← {t("blame.previous")}
          </button>
        )}
        <h2 className={styles.title}>
          {t("blame.title")}{" "}
          <span className={`${styles.path} selectable`}>{path}</span>{" "}
          {t("blame.at")} <code className={styles.sha}>{short}</code>
        </h2>
        {running && lines && total > 0 && (
          <>
            <progress
              className={styles.progress}
              max={total}
              value={attributed}
              aria-label={t("blame.progress")}
            />
            <span className={styles.counter} aria-live="polite">
              {t("blame.attributed", {
                count: total,
                done: attributed.toLocaleString(i18n.language),
                total: total.toLocaleString(i18n.language),
                percent,
              })}
            </span>
            <button
              type="button"
              className={styles.button}
              onClick={() => void commands.cancelBlame(repoId)}
            >
              {t("blame.cancel")}
            </button>
          </>
        )}
      </header>
      {notice}
      {lines && (
        <div
          ref={scrollRef}
          className={styles.scroller}
          role="table"
          aria-label={t("blame.tableLabel", { path })}
        >
          <div
            className={styles.lines}
            style={{ height: virtualizer.getTotalSize() }}
          >
            {virtualizer.getVirtualItems().map((item) => {
              const index = owner[item.index];
              const chunk = index === undefined ? undefined : chunks[index];
              const commit = chunk ? commits.get(chunk.oid) : undefined;
              const starts = chunk?.line === item.index + 1;
              const isChosen = !!chunk && chunk.oid === chosen;
              return (
                <div
                  key={item.index}
                  role="row"
                  className={[
                    styles.line,
                    chunk && band.get(index!) === 1 && styles.band,
                    !chunk && styles.pending,
                    isChosen && styles.chosen,
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  style={{ transform: `translateY(${item.start}px)` }}
                >
                  <div role="cell" className={styles.gutter}>
                    {chunk && starts && commit && (
                      <button
                        type="button"
                        className={styles.chunk}
                        data-chunk-line={item.index}
                        aria-pressed={isChosen}
                        aria-haspopup="menu"
                        aria-label={t("blame.chunkLabel", {
                          short: chunk.oid.slice(0, 7),
                          author: commit.authorName,
                          when: relativeTime(
                            commit.authorTime,
                            now,
                            i18n.language,
                          ),
                          first: chunk.line,
                          last: chunk.line + chunk.count - 1,
                          summary: commit.summary,
                        })}
                        title={commit.summary}
                        onClick={() => choose(repoId, chunk.oid)}
                        onContextMenu={(event) => {
                          event.preventDefault();
                          choose(repoId, chunk.oid);
                          setMenu({
                            chunk,
                            x: event.clientX,
                            y: event.clientY,
                          });
                        }}
                        onKeyDown={(event) => {
                          const key = whichShortcut(
                            ["blameNext", "blamePrevious", "commitMenu"],
                            event,
                          );
                          if (!key) return;
                          event.preventDefault();
                          if (key === "commitMenu") {
                            openMenu(chunk, event.currentTarget);
                            return;
                          }
                          const line = neighbour(
                            item.index,
                            key === "blameNext" ? "next" : "previous",
                          );
                          const target = owner[line];
                          if (line < 0 || target === undefined) return;
                          const next = chunks[target];
                          if (next) choose(repoId, next.oid);
                          focusLine(line);
                        }}
                      >
                        <code className={styles.sha}>
                          {chunk.oid.slice(0, 7)}
                        </code>{" "}
                        <span className={styles.author}>
                          {commit.authorName}
                        </span>{" "}
                        <span
                          className={styles.when}
                          title={absoluteTime(commit.authorTime, i18n.language)}
                        >
                          {relativeTime(commit.authorTime, now, i18n.language)}
                        </span>
                      </button>
                    )}
                  </div>
                  <div role="cell" className={styles.number}>
                    {item.index + 1}
                  </div>
                  <div role="cell" className={`${styles.text} selectable`}>
                    {lines[item.index]}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
      {lines && (
        <div className={styles.handleLayer}>
          <WidthHandle
            label={t("blame.resize")}
            value={width}
            measure={() =>
              scrollRef.current
                ?.querySelector<HTMLElement>("[role=cell]")
                ?.getBoundingClientRect().width ?? 260
            }
            max={() =>
              (viewRef.current?.getBoundingClientRect().width ?? 900) - 200
            }
            onChange={setWidth}
          />
        </div>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t("blame.chunkMenu", { short: menu.chunk.oid.slice(0, 7) })}
          items={[
            {
              label: t("blame.showInGraph"),
              onSelect: () => {
                const oid = menu.chunk.oid;
                close(repoId);
                select(repoId, oid);
              },
            },
            {
              label: t("blame.before"),
              disabled: !menu.chunk.previous,
              hint: menu.chunk.previous ? undefined : t("blame.firstCommit"),
              onSelect: () => {
                const previous = menu.chunk.previous;
                if (previous) before(repoId, previous.path, previous.oid);
              },
            },
            {
              label: t("blame.fileHistory"),
              onSelect: () =>
                open(repoId, {
                  kind: "history",
                  path: menu.chunk.path,
                  rev: menu.chunk.oid,
                  chosen: null,
                }),
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </section>
  );
}
