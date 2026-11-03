import { useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  ConflictEntry,
  StatusEntry,
  WorkingStatus,
  WorktreeArea,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useRememberedScroll } from "../repo/viewState";
import { changeCounts, useWorkdirStore } from "./store";
import { CommitBox } from "./CommitBox";
import { ResizeHandle } from "./ResizeHandle";
import { useWorkdirViewPrefs } from "./viewPrefs";
import { StatusLetter } from "./StatusLetter";
import styles from "./workdir.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

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
            title={t("workdir.conflicts")}
            count={status.conflicted.length}
          >
            {status.conflicted.map((c) => (
              <ConflictRow key={c.path} entry={c} />
            ))}
          </Section>
        )}
        <Section
          title={t("workdir.unstaged")}
          count={unstaged.length}
          action={
            <>
              <button
                type="button"
                className={`${styles.action} ${styles.danger}`}
                disabled={busy}
                onClick={() =>
                  void requestDiscard(repoId, {
                    kind: "files",
                    paths: unstaged.map((e) => e.path),
                  })
                }
              >
                {t("workdir.discardAll")}
              </button>
              <button
                type="button"
                className={styles.action}
                disabled={busy}
                onClick={() => void stage(repoId, null)}
              >
                {t("workdir.stageAll")}
              </button>
            </>
          }
        >
          {unstaged.map((e) => (
            <FileRow
              key={`u:${e.path}`}
              repoId={repoId}
              area={e.status === "untracked" ? "untracked" : "unstaged"}
              entry={e}
              actionLabel={t("workdir.stage")}
              actionTitle={t("workdir.stageFile", { path: e.path })}
              disabled={busy}
              onAction={() => void stage(repoId, [e.path])}
              onDiscard={() =>
                void requestDiscard(repoId, { kind: "files", paths: [e.path] })
              }
            />
          ))}
        </Section>
        <Section
          title={t("workdir.staged")}
          count={status.staged.length}
          action={
            <button
              type="button"
              className={styles.action}
              disabled={busy}
              onClick={() => void unstage(repoId, null)}
            >
              {t("workdir.unstageAll")}
            </button>
          }
        >
          {status.staged.map((e) => (
            <FileRow
              key={`s:${e.path}`}
              repoId={repoId}
              area="staged"
              entry={e}
              actionLabel={t("workdir.unstage")}
              actionTitle={t("workdir.unstageFile", { path: e.path })}
              disabled={busy}
              // A staged rename lives under both names in the index.
              onAction={() =>
                void unstage(repoId, e.oldPath ? [e.path, e.oldPath] : [e.path])
              }
            />
          ))}
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

function Section({
  title,
  count,
  action,
  children,
}: {
  title: string;
  count: number;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <section className={styles.section} aria-label={title}>
      <h3 className={styles.sectionTitle}>
        {title} <span className={styles.count}>{count}</span>
        {count > 0 && action && (
          <span className={styles.sectionAction}>{action}</span>
        )}
      </h3>
      {count === 0 ? (
        <p className={styles.muted}>{t("workdir.none")}</p>
      ) : (
        <ul className={styles.list}>{children}</ul>
      )}
    </section>
  );
}

export function PathLabel({
  path,
  oldPath,
}: {
  path: string;
  oldPath?: string | null;
}) {
  const { t } = useTranslation();
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const slash = trimmed.lastIndexOf("/");
  const dir = slash >= 0 ? path.slice(0, slash + 1) : "";
  const name = path.slice(slash + 1);
  return (
    <span className={styles.path} title={path}>
      {dir && <span className={styles.dir}>{dir}</span>}
      <span>{name}</span>
      {oldPath && (
        <span className={styles.oldPath}>
          {t("files.renamedFrom", { path: oldPath })}
        </span>
      )}
    </span>
  );
}

function FileRow({
  repoId,
  area,
  entry,
  actionLabel,
  actionTitle,
  disabled,
  onAction,
  onDiscard,
}: {
  repoId: number;
  area: WorktreeArea;
  entry: StatusEntry;
  actionLabel: string;
  actionTitle: string;
  disabled: boolean;
  onAction: () => void;
  /** Unstaged and untracked files only. */
  onDiscard?: () => void;
}) {
  const { t } = useTranslation();
  const open = useGraphStore((s) => s.openFileDiff);
  const isOpen = useGraphStore((s) => {
    const f = s.graphs[repoId]?.openFile;
    return f?.kind === "worktree" && f.area === area && f.path === entry.path;
  });
  // An untracked directory has no single-file diff.
  const hasDiff = !entry.path.endsWith("/");
  return (
    <li
      className={isOpen ? `${styles.file} ${styles.fileOpen}` : styles.file}
      onDoubleClick={disabled ? undefined : onAction}
    >
      <StatusLetter status={entry.status} />
      {hasDiff ? (
        <button
          type="button"
          className={styles.pathButton}
          aria-label={t("workdir.viewChanges", { path: entry.path })}
          aria-current={isOpen || undefined}
          onClick={() =>
            open(repoId, {
              kind: "worktree",
              area,
              path: entry.path,
              oldPath: entry.oldPath,
            })
          }
        >
          <PathLabel path={entry.path} oldPath={entry.oldPath} />
        </button>
      ) : (
        <PathLabel path={entry.path} oldPath={entry.oldPath} />
      )}
      {onDiscard && (
        <button
          type="button"
          className={`${styles.rowAction} ${styles.danger}`}
          aria-label={t("workdir.discardFile", { path: entry.path })}
          title={t("workdir.discardFile", { path: entry.path })}
          disabled={disabled}
          onClick={onDiscard}
        >
          {t("workdir.discard")}
        </button>
      )}
      <button
        type="button"
        className={styles.rowAction}
        aria-label={actionTitle}
        title={actionTitle}
        disabled={disabled}
        onClick={onAction}
      >
        {actionLabel}
      </button>
    </li>
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
