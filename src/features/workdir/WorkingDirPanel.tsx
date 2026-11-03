import { useId, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ConflictEntry, StatusEntry, WorkingStatus } from "../../ipc";
import { useRememberedScroll } from "../repo/viewState";
import { changeCounts, useWorkdirStore } from "./store";
import {
  ChangeList,
  dirKey,
  PathLabel,
  type ChangeSection,
} from "./ChangeList";
import { CommitBox } from "./CommitBox";
import { buildTree, dirPaths } from "./fileTree";
import { ResizeHandle } from "./ResizeHandle";
import { useWorkdirViewPrefs, type WorkdirSection } from "./viewPrefs";
import styles from "./workdir.module.css";
import { Chevron } from "../../ui/Chevron";
import { ErrorReport } from "../../ui/ErrorReport";
import { Tooltip } from "../../ui/Tooltip";

/** Smallest commit box, and the room always left for the list above it. */
export const COMMIT_BOX_MIN = 140;
const LIST_MIN = 120;

/**
 * Right-hand panel for the "uncommitted changes" entry: a header, the
 * changes (scrolling), a divider and the commit box fixed at the bottom.
 */
export function WorkingDirPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const panel = useRef<HTMLDivElement>(null);
  const changes = useRef<HTMLDivElement>(null);
  const commitArea = useRef<HTMLDivElement>(null);
  const height = useWorkdirViewPrefs((s) => s.commitBoxHeight);
  const setHeight = useWorkdirViewPrefs((s) => s.setCommitBoxHeight);
  useRememberedScroll(changes, `${repoId}:workdir`);
  const entry = useWorkdirStore((s) => s.statuses[repoId]);
  const stage = useWorkdirStore((s) => s.stage);
  const unstage = useWorkdirStore((s) => s.unstage);
  const requestDiscard = useWorkdirStore((s) => s.requestDiscard);
  const dismissError = useWorkdirStore((s) => s.dismissError);
  const status = entry?.status ?? null;
  const busy = entry?.busy ?? false;
  // The divider's limits depend on the panel's height: follow it.
  const [, setPanelHeight] = useState(0);
  const hasStatus = status !== null;
  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    setPanelHeight(element.clientHeight);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() =>
      setPanelHeight(element.clientHeight),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [hasStatus]);

  if (!status) {
    return (
      <div className={styles.panelMessage}>
        {entry?.error ? (
          <ErrorReport error={entry.error} lead={t("workdir.failed")} />
        ) : (
          <p className={styles.muted}>{t("workdir.loading")}</p>
        )}
      </div>
    );
  }

  const unstaged = [...status.unstaged, ...status.untracked];
  return (
    <div ref={panel} className={styles.panel}>
      <header className={styles.header}>
        <h2 className={styles.title}>{t("workdir.row")}</h2>
        <p className={styles.headerSummary}>{headerSummary(t, status)}</p>
        <ViewControls repoId={repoId} status={status} />
      </header>
      <div ref={changes} className={styles.changes}>
        {entry?.opError && (
          <div className={styles.opError}>
            <ErrorReport error={entry.opError} lead={t("workdir.opFailed")} />
            <button
              type="button"
              className={styles.action}
              onClick={() => dismissError(repoId)}
            >
              {t("errors.dismiss")}
            </button>
          </div>
        )}
        {status.conflicted.length > 0 && (
          <Section
            id="conflicts"
            title={t("workdir.conflicts")}
            count={status.conflicted.length}
          >
            <ul className={styles.list}>
              {status.conflicted.map((c) => (
                <ConflictRow key={c.path} entry={c} />
              ))}
            </ul>
          </Section>
        )}
        <Section
          id="unstaged"
          title={t("workdir.unstaged")}
          count={unstaged.length}
          actions={
            <>
              <button
                type="button"
                className={styles.action}
                disabled={busy || unstaged.length === 0}
                onClick={() => void stage(repoId, null)}
              >
                {t("workdir.stageAll")}
              </button>
              <button
                type="button"
                className={`${styles.action} ${styles.danger}`}
                disabled={busy || unstaged.length === 0}
                onClick={() =>
                  void requestDiscard(repoId, {
                    kind: "files",
                    paths: unstaged.map((e) => e.path),
                  })
                }
              >
                {t("workdir.discardAll")}
              </button>
            </>
          }
        >
          <ChangeList
            repoId={repoId}
            section="unstaged"
            label={t("workdir.unstaged")}
            entries={unstaged}
            disabled={busy}
          />
        </Section>
        <Section
          id="staged"
          title={t("workdir.staged")}
          count={status.staged.length}
          actions={
            <button
              type="button"
              className={styles.action}
              disabled={busy || status.staged.length === 0}
              onClick={() => void unstage(repoId, null)}
            >
              {t("workdir.unstageAll")}
            </button>
          }
        >
          <ChangeList
            repoId={repoId}
            section="staged"
            label={t("workdir.staged")}
            entries={status.staged}
            disabled={busy}
          />
        </Section>
      </div>
      <ResizeHandle
        label={t("workdir.resizeCommitBox")}
        value={height}
        measure={() => commitArea.current?.offsetHeight ?? COMMIT_BOX_MIN}
        min={COMMIT_BOX_MIN}
        max={() => (panel.current?.clientHeight ?? 0) - LIST_MIN}
        onChange={setHeight}
      />
      <div
        ref={commitArea}
        className={styles.commitArea}
        style={height === null ? undefined : { height }}
      >
        <CommitBox repoId={repoId} status={status} />
      </div>
    </div>
  );
}

/** "3 changes on main" (or on a detached HEAD). */
function headerSummary(
  t: ReturnType<typeof useTranslation>["t"],
  status: WorkingStatus,
): string {
  const count = changeCounts(status).total;
  const branch = status.branch.name;
  return branch
    ? t("workdir.summaryOnBranch", { count, branch })
    : t("workdir.summaryDetached", { count });
}

/** A collapsible group of changes with its count and its actions. */
function Section({
  id,
  title,
  count,
  actions,
  children,
}: {
  id: WorkdirSection;
  title: string;
  count: number;
  /** Always shown (disabled when there is nothing to act on). */
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  const { t } = useTranslation();
  const collapsed = useWorkdirViewPrefs((s) => !!s.collapsedSections[id]);
  const toggle = useWorkdirViewPrefs((s) => s.toggleSection);
  const bodyId = useId();
  return (
    <section className={styles.section} aria-label={title}>
      <h3 className={styles.sectionTitle}>
        <button
          type="button"
          className={styles.sectionToggle}
          aria-expanded={!collapsed}
          aria-controls={bodyId}
          onClick={() => toggle(id)}
        >
          <Chevron direction={collapsed ? "right" : "down"} />
          {title} <span className={styles.count}>{count}</span>
        </button>
        {actions && <span className={styles.sectionAction}>{actions}</span>}
      </h3>
      <div id={bodyId} hidden={collapsed}>
        {count === 0 ? (
          <p className={styles.muted}>{t("workdir.none")}</p>
        ) : (
          children
        )}
      </div>
    </section>
  );
}

/** List or tree, the order, and in the tree "expand all" / "collapse all". */
function ViewControls({
  repoId,
  status,
}: {
  repoId: number;
  status: WorkingStatus;
}) {
  const { t } = useTranslation();
  const layout = useWorkdirViewPrefs((s) => s.layout);
  const setLayout = useWorkdirViewPrefs((s) => s.setLayout);
  const setDirsCollapsed = useWorkdirViewPrefs((s) => s.setDirsCollapsed);
  const sort = useWorkdirViewPrefs((s) => s.sort);
  const setSort = useWorkdirViewPrefs((s) => s.setSort);
  const sections: [ChangeSection, StatusEntry[]][] = [
    ["unstaged", [...status.unstaged, ...status.untracked]],
    ["staged", status.staged],
  ];
  const allDirs = sections.flatMap(([section, entries]) =>
    dirPaths(buildTree(entries)).map((path) => dirKey(repoId, section, path)),
  );
  return (
    <div className={styles.viewControls}>
      <div
        className={styles.segmented}
        role="group"
        aria-label={t("workdir.view.label")}
      >
        {(["list", "tree"] as const).map((value) => (
          <button
            key={value}
            type="button"
            className={styles.segment}
            aria-pressed={layout === value}
            onClick={() => setLayout(value)}
          >
            {t(`workdir.view.${value}`)}
          </button>
        ))}
      </div>
      <div
        className={styles.segmented}
        role="group"
        aria-label={t("workdir.sort.label")}
      >
        {(["name", "status"] as const).map((value) => (
          <button
            key={value}
            type="button"
            className={styles.segment}
            aria-pressed={sort === value}
            onClick={() => setSort(value)}
          >
            {t(`workdir.sort.${value}`)}
          </button>
        ))}
      </div>
      {layout === "tree" && (
        <>
          <IconButton
            label={t("workdir.expandAll")}
            disabled={allDirs.length === 0}
            onClick={() => setDirsCollapsed(allDirs, false)}
            path="M6 1v3.5M4 2.5 6 .8 8 2.5M6 11V7.5M4 9.5l2 1.7 2-1.7M2 6h8"
          />
          <IconButton
            label={t("workdir.collapseAll")}
            disabled={allDirs.length === 0}
            onClick={() => setDirsCollapsed(allDirs, true)}
            path="M6 .5V4M4 2.3 6 4l2-1.7M6 11.5V8M4 9.7 6 8l2 1.7M2 6h8"
          />
        </>
      )}
    </div>
  );
}

/** A small square button with an icon; its name is also its tooltip. */
function IconButton({
  label,
  path,
  disabled,
  onClick,
}: {
  label: string;
  /** SVG path in a 12×12 box. */
  path: string;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <Tooltip label={label}>
      <button
        type="button"
        className={`${styles.action} ${styles.iconAction}`}
        aria-label={label}
        disabled={disabled}
        onClick={onClick}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true" focusable="false">
          <path d={path} />
        </svg>
      </button>
    </Tooltip>
  );
}

function ConflictRow({ entry }: { entry: ConflictEntry }) {
  const { t } = useTranslation();
  return (
    <li className={styles.file}>
      <span
        className={`${styles.letter} ${styles.conflict}`}
        aria-hidden="true"
      >
        !
      </span>
      <PathLabel path={entry.path} />
      <span className={styles.conflictKind}>
        {t(`workdir.conflict.${entry.kind}`)}
      </span>
    </li>
  );
}
