import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isShortcut, whichShortcut } from "../../app/shortcuts";
import {
  commands,
  fileHistory,
  type CommandError,
  type FileHistoryEntry,
  type WorktreeArea,
} from "../../ipc";
import { ContextMenu } from "../../ui/ContextMenu";
import { ErrorReport } from "../../ui/ErrorReport";
import { DiffContent, DiffTitle } from "../diff/DiffView";
import { headOid } from "../graph/refLabels";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { useGraphStore, WORKTREE, type DiffFile } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";
import { WidthHandle } from "./WidthHandle";
import styles from "./fileHistory.module.css";

const ROW_HEIGHT = 46;

type Loading = {
  entries: FileHistoryEntry[];
  done: boolean;
  error: CommandError | null;
};

/** The uncommitted changes of `path` (staged, unstaged or new), if any. */
function useUncommitted(repoId: number, path: string): WorktreeArea[] {
  const status = useWorkdirStore((s) => s.statuses[repoId]?.status ?? null);
  return useMemo(() => {
    if (!status) return [];
    const areas: WorktreeArea[] = [];
    if (status.staged.some((e) => e.path === path)) areas.push("staged");
    if (status.unstaged.some((e) => e.path === path)) areas.push("unstaged");
    if (status.untracked.some((e) => e.path === path)) areas.push("untracked");
    return areas;
  }, [status, path]);
}

/**
 * The history of one file (task 12.4), over the graph: the commits that
 * changed it, newest first and following renames, beside the diff of the
 * chosen one. Its uncommitted changes, if any, come first. "Back to the
 * graph" or Esc closes it; the graph keeps its selection and scroll.
 */
export function FileHistoryView({
  repoId,
  path,
  rev,
  chosen,
}: {
  repoId: number;
  path: string;
  rev: string;
  chosen: string | null;
}) {
  const { t, i18n } = useTranslation();
  const close = useGraphStore((s) => s.closeFile);
  const choose = useGraphStore((s) => s.chooseInHistory);
  const select = useGraphStore((s) => s.select);
  const head = useGraphStore((s) => headOid(s.graphs[repoId]?.refs ?? null));
  const [loading, setLoading] = useState<Loading>({
    entries: [],
    done: false,
    error: null,
  });
  const [width, setWidth] = useState<number | null>(null);
  const [menu, setMenu] = useState<{
    entry: FileHistoryEntry;
    x: number;
    y: number;
  } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<HTMLElement>(null);
  const now = Date.now();

  // Uncommitted changes only make sense from the current commit.
  const fromHead = rev === "HEAD" || rev === head;
  const areas = useUncommitted(repoId, path);
  const showUncommitted = fromHead && areas.length > 0;

  useEffect(() => {
    let stale = false;
    setLoading({ entries: [], done: false, error: null });
    void fileHistory(repoId, path, rev, (batch) => {
      if (stale) return;
      setLoading((l) => ({ ...l, entries: l.entries.concat(batch) }));
    }).then((result) => {
      if (stale) return;
      setLoading((l) =>
        result.status === "ok"
          ? { ...l, done: true }
          : result.error.kind === "cancelled"
            ? { ...l, done: true }
            : { ...l, done: true, error: result.error },
      );
    });
    return () => {
      stale = true;
      void commands.cancelFileHistory(repoId);
    };
  }, [repoId, path, rev]);

  const { entries, done, error } = loading;

  // The first row is chosen when the view opens.
  useEffect(() => {
    if (chosen !== null) return;
    if (showUncommitted) choose(repoId, WORKTREE);
    else if (entries[0]) choose(repoId, entries[0].oid);
  }, [chosen, showUncommitted, entries, choose, repoId]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // Esc closes an open menu first (the menu handles it).
      if (document.querySelector('[role="menu"], [aria-modal="true"]')) return;
      if (isShortcut("escape", event)) close(repoId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId, close]);

  // Rows: the uncommitted changes (if any), then the commits.
  const rows: (FileHistoryEntry | typeof WORKTREE)[] = useMemo(
    () => (showUncommitted ? [WORKTREE, ...entries] : entries),
    [showUncommitted, entries],
  );
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  const idOf = (row: FileHistoryEntry | typeof WORKTREE) =>
    row === WORKTREE ? WORKTREE : row.oid;
  const chosenIndex = rows.findIndex((row) => idOf(row) === chosen);
  const chosenEntry =
    chosenIndex >= 0 && rows[chosenIndex] !== WORKTREE
      ? (rows[chosenIndex] as FileHistoryEntry)
      : null;

  const moveTo = (index: number) => {
    const row = rows[index];
    if (!row) return;
    choose(repoId, idOf(row));
    virtualizer.scrollToIndex(index, { align: "auto" });
    // Keep the keyboard focus on the chosen row.
    requestAnimationFrame(() =>
      listRef.current
        ?.querySelector<HTMLElement>(`[data-index="${index}"]`)
        ?.focus(),
    );
  };

  const openMenu = (entry: FileHistoryEntry, element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setMenu({ entry, x: rect.left + 24, y: rect.bottom });
  };

  const counter = done
    ? t("fileHistory.count", { count: entries.length })
    : t("fileHistory.loading", { count: entries.length });

  return (
    <section
      ref={viewRef}
      className={styles.view}
      aria-label={t("fileHistory.label", { path })}
    >
      <header className={styles.header}>
        <button
          type="button"
          className={styles.back}
          title={t("fileHistory.backHint")}
          onClick={() => close(repoId)}
        >
          ← {t("fileHistory.back")}
        </button>
        <h2 className={styles.title}>
          {t("fileHistory.title")}{" "}
          <span className={`${styles.path} selectable`}>{path}</span>
        </h2>
        <span className={styles.counter} aria-live="polite">
          {counter}
        </span>
        {!done && (
          <button
            type="button"
            className={styles.button}
            onClick={() => void commands.cancelFileHistory(repoId)}
          >
            {t("fileHistory.cancel")}
          </button>
        )}
      </header>
      <div className={styles.body}>
        <div
          className={styles.listPane}
          style={width === null ? undefined : { width }}
        >
          {error && (
            <ErrorReport error={error} lead={t("fileHistory.failed")} />
          )}
          {done && !error && rows.length === 0 && (
            <p className={styles.empty}>{t("fileHistory.empty")}</p>
          )}
          <div
            ref={listRef}
            className={styles.list}
            role="listbox"
            aria-label={t("fileHistory.listLabel", { path })}
          >
            <div
              style={{
                height: virtualizer.getTotalSize(),
                position: "relative",
              }}
            >
              {virtualizer.getVirtualItems().map((item) => {
                const row = rows[item.index];
                if (!row) return null;
                const isChosen = idOf(row) === chosen;
                const common = {
                  role: "option",
                  "aria-selected": isChosen,
                  tabIndex:
                    isChosen || (chosenIndex < 0 && item.index === 0) ? 0 : -1,
                  "data-index": item.index,
                  style: { transform: `translateY(${item.start}px)` },
                  onClick: () => choose(repoId, idOf(row)),
                  onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => {
                    const key = whichShortcut(
                      ["fileHistoryNext", "fileHistoryPrevious", "commitMenu"],
                      event,
                    );
                    if (!key) return;
                    event.preventDefault();
                    if (key === "fileHistoryNext") moveTo(item.index + 1);
                    else if (key === "fileHistoryPrevious")
                      moveTo(item.index - 1);
                    else if (row !== WORKTREE)
                      openMenu(row, event.currentTarget);
                  },
                } as const;
                if (row === WORKTREE) {
                  return (
                    <div
                      key={WORKTREE}
                      {...common}
                      className={[
                        styles.row,
                        styles.uncommitted,
                        isChosen && styles.chosen,
                      ]
                        .filter(Boolean)
                        .join(" ")}
                    >
                      <span className={styles.summary}>
                        {t("fileHistory.uncommitted")}
                      </span>
                      <span className={styles.meta}>
                        {areas
                          .map((area) => t(`fileHistory.area.${area}`))
                          .join(" · ")}
                      </span>
                    </div>
                  );
                }
                return (
                  <div
                    key={row.oid}
                    {...common}
                    className={[styles.row, isChosen && styles.chosen]
                      .filter(Boolean)
                      .join(" ")}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      choose(repoId, row.oid);
                      setMenu({
                        entry: row,
                        x: event.clientX,
                        y: event.clientY,
                      });
                    }}
                  >
                    <span className={styles.summary}>{row.summary}</span>
                    <span className={styles.meta}>
                      <span>{row.authorName}</span>
                      {" · "}
                      <span title={absoluteTime(row.authorTime, i18n.language)}>
                        {relativeTime(row.authorTime, now, i18n.language)}
                      </span>
                      {" · "}
                      <span className={styles.change}>
                        {row.status === "renamed" && row.oldPath
                          ? t("fileHistory.renamedFrom", { path: row.oldPath })
                          : t(`fileHistory.status.${row.status}`)}
                      </span>
                      {row.path !== path && row.status !== "renamed" && (
                        <span className={styles.otherPath}> · {row.path}</span>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
        <WidthHandle
          label={t("fileHistory.resize")}
          value={width}
          measure={() =>
            listRef.current?.parentElement?.getBoundingClientRect().width ?? 320
          }
          max={() =>
            (viewRef.current?.getBoundingClientRect().width ?? 900) - 240
          }
          onChange={setWidth}
        />
        <div className={styles.diffPane}>
          {chosen === WORKTREE ? (
            areas.map((area) => (
              <section
                key={area}
                className={styles.diffSection}
                aria-label={t(`fileHistory.area.${area}`)}
              >
                <header className={styles.diffHeader}>
                  <DiffTitle file={worktreeFile(path, area)} />
                </header>
                <DiffContent repoId={repoId} file={worktreeFile(path, area)} />
              </section>
            ))
          ) : chosenEntry ? (
            <section className={styles.diffSection}>
              <header className={styles.diffHeader}>
                <DiffTitle file={commitFile(chosenEntry)} />
              </header>
              <DiffContent repoId={repoId} file={commitFile(chosenEntry)} />
            </section>
          ) : null}
        </div>
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t("fileHistory.entryMenu", {
            short: menu.entry.oid.slice(0, 7),
          })}
          items={[
            {
              label: t("fileHistory.showInGraph"),
              onSelect: () => {
                const oid = menu.entry.oid;
                close(repoId);
                select(repoId, oid);
              },
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </section>
  );
}

function worktreeFile(path: string, area: WorktreeArea): DiffFile {
  return { kind: "worktree", area, path, oldPath: null };
}

function commitFile(entry: FileHistoryEntry): DiffFile {
  return {
    kind: "commit",
    oid: entry.oid,
    path: entry.path,
    oldPath: entry.status === "renamed" ? entry.oldPath : null,
  };
}
