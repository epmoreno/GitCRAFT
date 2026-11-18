import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isShortcut, whichShortcut } from "../../app/shortcuts";
import { useShortcutLabel } from "../../app/useShortcutLabel";
import type { PlanPreview, RangeCommit, StepAction } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { useWorkdirStore } from "../workdir/store";
import {
  chainLines,
  chainMembers,
  combinedMessage,
  isOriginal,
  messageOwner,
  rolesOf,
  type ChainLine,
} from "./plan";
import { useInteractiveRebaseStore, type RebaseView } from "./store";
import styles from "./interactiveRebase.module.css";

/** Height of a row of the list (fixed: dragging measures with it). */
export const ROW_HEIGHT = 34;
/** The pointer moves this much before a row is dragged. */
const DRAG_THRESHOLD = 4;
/** Most commits one interactive rebase takes (the core's `MAX_COMMITS`). */
const MAX_COMMITS = 500;

const ACTIONS: StepAction[] = ["pick", "reword", "squash", "drop"];

const KEYS = [
  "rebaseNext",
  "rebasePrevious",
  "rebaseMoveUp",
  "rebaseMoveDown",
  "rebasePick",
  "rebaseReword",
  "rebaseSquash",
  "rebaseDrop",
] as const;

const ACTION_KEYS: Record<string, StepAction> = {
  rebasePick: "pick",
  rebaseReword: "reword",
  rebaseSquash: "squash",
  rebaseDrop: "drop",
};

const LINE_CLASS: Record<ChainLine, string | undefined> = {
  none: undefined,
  start: styles.chainStart,
  middle: styles.chainMiddle,
  end: styles.chainEnd,
};

/**
 * The interactive rebase of the current branch from one of its commits
 * (task 13.4), over the history: the commits newest first, each with its
 * action; reordered by dragging or with the keyboard; the message of a
 * reworded or combined commit; what git would do; and "Apply…".
 */
export function InteractiveRebaseView({ repoId }: { repoId: number }) {
  const view = useInteractiveRebaseStore((s) =>
    s.view?.repoId === repoId ? s.view : null,
  );
  const reload = useInteractiveRebaseStore((s) => s.reload);
  // The repository changed (a commit, a file saved…): a new preview.
  const version = useWorkdirStore((s) => s.statuses[repoId]?.version ?? 0);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    void reload();
  }, [version, reload]);

  if (!view) return null;
  return <RebaseViewBody view={view} />;
}

function RebaseViewBody({ view }: { view: RebaseView }) {
  const { t } = useTranslation();
  const label = useShortcutLabel();
  const close = useInteractiveRebaseStore((s) => s.close);
  const preview = view.preview;
  const branch = preview?.branch ?? "";

  // Esc goes back to the graph; in the message, back to the list first.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (document.querySelector('[role="menu"], [aria-modal="true"]')) return;
      if (!isShortcut("escape", event)) return;
      const active = document.activeElement;
      if (active instanceof HTMLTextAreaElement && active.dataset.message) {
        event.preventDefault();
        document
          .querySelector<HTMLElement>('[data-rebase-row][tabindex="0"]')
          ?.focus();
        return;
      }
      close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  return (
    <section
      className={styles.view}
      aria-label={t("interactiveRebase.title", { branch })}
    >
      <header className={styles.header}>
        <h2 className={styles.title}>
          {t("interactiveRebase.title", { branch })}
        </h2>
        <button
          type="button"
          className={styles.button}
          title={t("interactiveRebase.backHint", {
            shortcut: label("escape"),
          })}
          onClick={close}
        >
          <span aria-hidden="true">✕ </span>
          {t("interactiveRebase.back")}
        </button>
      </header>
      {!preview ? (
        view.error ? (
          <ErrorReport
            className={styles.notice}
            error={view.error}
            lead={t("interactiveRebase.failed")}
          />
        ) : (
          <p className={styles.notice}>{t("interactiveRebase.loading")}</p>
        )
      ) : preview.refused ? (
        <div className={styles.notice} role="alert">
          <p>
            {t(`interactiveRebase.refused.${preview.refused}`, {
              branch,
              max: MAX_COMMITS,
            })}
          </p>
          <button type="button" className={styles.button} onClick={close}>
            {t("interactiveRebase.back")}
          </button>
        </div>
      ) : (
        <Plan view={view} />
      )}
    </section>
  );
}

function Plan({ view }: { view: RebaseView }) {
  const { t, i18n } = useTranslation();
  const label = useShortcutLabel();
  const store = useInteractiveRebaseStore.getState;
  const busy = useInteractiveRebaseStore((s) => s.busy);
  const preview = view.preview!;
  const { rows, selected, prediction, predicting } = view;
  const branch = preview.branch ?? "";
  // Relative dates as of when the view opened.
  const [now] = useState(() => Date.now());

  const commits = useMemo(
    () =>
      new Map<string, RangeCommit>(
        preview.commits.map((c) => [c.commit.oid, c]),
      ),
    [preview.commits],
  );
  const roles = useMemo(() => rolesOf(rows), [rows]);
  const lines = useMemo(() => chainLines(rows, roles), [rows, roles]);
  const shown = predicting ? null : prediction;
  const empty = useMemo(
    () => new Set(shown?.empty.map((c) => c.oid) ?? []),
    [shown],
  );

  const listRef = useRef<HTMLDivElement>(null);
  const editorId = useId();
  // Asks for the focus on a row once it is rendered where it now is. The
  // chosen commit has it when the list appears.
  const [focus, setFocus] = useState<{ oid: string | null }>(() => ({
    oid: useInteractiveRebaseStore.getState().view?.selected ?? null,
  }));
  const setFocusOid = (oid: string) => setFocus({ oid });
  useEffect(() => {
    if (!focus.oid) return;
    listRef.current
      ?.querySelector<HTMLElement>(`[data-rebase-row="${focus.oid}"]`)
      ?.focus();
  }, [focus]);

  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const stopDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => stopDrag.current?.(), []);

  const startDrag = (event: React.PointerEvent, from: number) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const startY = event.clientY;
    let started = false;
    const indexAt = (y: number) => {
      const list = listRef.current;
      const top = list?.getBoundingClientRect().top ?? 0;
      const index = Math.floor((y - top + (list?.scrollTop ?? 0)) / ROW_HEIGHT);
      return Math.max(0, Math.min(rows.length - 1, index));
    };
    const onMove = (e: PointerEvent) => {
      if (!started && Math.abs(e.clientY - startY) < DRAG_THRESHOLD) return;
      started = true;
      setDrag({ from, to: indexAt(e.clientY) });
    };
    const onUp = (e: PointerEvent) => {
      stop();
      if (started) {
        const oid = rows[from]?.oid;
        store().moveTo(from, indexAt(e.clientY));
        if (oid) {
          store().select(oid);
          setFocusOid(oid);
        }
      }
    };
    const stop = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", stop);
      stopDrag.current = null;
      setDrag(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", stop);
    stopDrag.current = stop;
    store().select(rows[from]!.oid);
  };

  const onRowKey = (event: React.KeyboardEvent, index: number) => {
    // The action's own menu keeps its keys.
    if (event.target !== event.currentTarget) return;
    const key = whichShortcut(KEYS, event);
    if (!key) return;
    const row = rows[index]!;
    const action = ACTION_KEYS[key];
    if (action) {
      if (!preview.messages && (action === "reword" || action === "squash")) {
        return;
      }
      event.preventDefault();
      store().setAction(row.oid, action);
      return;
    }
    event.preventDefault();
    if (key === "rebaseNext" || key === "rebasePrevious") {
      const next = rows[index + (key === "rebaseNext" ? 1 : -1)];
      if (!next) return;
      store().select(next.oid);
      setFocusOid(next.oid);
      return;
    }
    store().move(row.oid, key === "rebaseMoveUp" ? -1 : 1);
    setFocusOid(row.oid);
  };

  const owner = selected ? messageOwner(rows, roles, selected) : null;
  const editor = (() => {
    if (!owner || !selected) return null;
    const short = (oid: string) => commits.get(oid)?.commit.short ?? "";
    if (owner.kind === "reword") {
      const row = rows.find((r) => r.oid === selected);
      return {
        label: t("interactiveRebase.messageReword", { short: short(selected) }),
        value: row?.message ?? "",
        note: null,
      };
    }
    const members = chainMembers(rows, roles, owner.receiver);
    return {
      label: t("interactiveRebase.messageCombined", {
        count: members.length,
        shorts: members.map(short).join(", "),
      }),
      value: combinedMessage(
        preview,
        rows,
        roles,
        view.combined,
        owner.receiver,
      ),
      note: t("interactiveRebase.messageRefill"),
    };
  })();

  const canApply = !!prediction && !predicting && !prediction.problem && !busy;
  const original =
    isOriginal(preview, rows) && Object.keys(view.combined).length === 0;

  return (
    <>
      <div className={styles.order}>
        <p>{t("interactiveRebase.order", { count: rows.length })}</p>
        <p>
          {t("interactiveRebase.keysHint", {
            up: label("rebaseMoveUp"),
            down: label("rebaseMoveDown"),
            first: label("rebasePick"),
            last: label("rebaseDrop"),
          })}
        </p>
        {!preview.messages && (
          <p className={styles.warning}>
            {t("interactiveRebase.oldGit", { version: preview.gitVersion })}
          </p>
        )}
      </div>
      <div
        ref={listRef}
        className={styles.list}
        role="list"
        aria-label={t("interactiveRebase.listLabel", { branch })}
      >
        {rows.map((row, index) => {
          const commit = commits.get(row.oid);
          if (!commit) return null;
          const role = roles.get(row.oid);
          const step = rows.length - index;
          const isSelected = row.oid === selected;
          const stops = shown?.stopsAt?.commit.oid === row.oid;
          const target =
            drag && drag.to === index && drag.from !== index
              ? drag.to < drag.from
                ? styles.dropAbove
                : styles.dropBelow
              : undefined;
          return (
            <div
              key={row.oid}
              role="listitem"
              aria-current={isSelected}
              aria-label={t("interactiveRebase.rowLabel", {
                step,
                short: commit.commit.short,
                subject: commit.commit.subject,
                action: t(`interactiveRebase.action.${row.action}`),
              })}
              tabIndex={isSelected ? 0 : -1}
              data-rebase-row={row.oid}
              className={[
                styles.row,
                isSelected && styles.selected,
                row.action === "drop" && styles.dropped,
                drag?.from === index && styles.dragging,
                target,
              ]
                .filter(Boolean)
                .join(" ")}
              onClick={() => store().select(row.oid)}
              onKeyDown={(event) => onRowKey(event, index)}
            >
              <span
                className={styles.grip}
                title={t("interactiveRebase.grip", {
                  short: commit.commit.short,
                })}
                aria-hidden="true"
                data-grip={row.oid}
                onPointerDown={(event) => startDrag(event, index)}
              >
                ⠿
              </span>
              <span
                className={styles.step}
                title={t("interactiveRebase.step", { step })}
              >
                {step}
              </span>
              <span
                className={[styles.chain, LINE_CLASS[lines[index]!]]
                  .filter(Boolean)
                  .join(" ")}
                aria-hidden="true"
              />
              <select
                className={styles.action}
                aria-label={t("interactiveRebase.actionLabel", {
                  short: commit.commit.short,
                })}
                value={row.action}
                onClick={(event) => event.stopPropagation()}
                onFocus={() => store().select(row.oid)}
                onChange={(event) =>
                  store().setAction(row.oid, event.target.value as StepAction)
                }
              >
                {ACTIONS.map((action) => (
                  <option
                    key={action}
                    value={action}
                    disabled={
                      !preview.messages &&
                      (action === "reword" || action === "squash")
                    }
                  >
                    {t(`interactiveRebase.action.${action}`)}
                  </option>
                ))}
              </select>
              <span
                className={styles.marker}
                title={
                  role?.kind === "combined"
                    ? t("interactiveRebase.combinedInto", {
                        short: commits.get(role.receiver)?.commit.short ?? "",
                      })
                    : undefined
                }
              >
                {role?.kind === "combined" ? "↓" : ""}
              </span>
              <code className={`${styles.sha} selectable`}>
                {commit.commit.short}
              </code>
              <span className={`${styles.subject} selectable`}>
                {commit.commit.subject}
              </span>
              {role?.kind === "receiver" && (
                <span className={styles.badge}>
                  {t("interactiveRebase.receives", { count: role.count })}
                </span>
              )}
              {role?.kind === "orphan" && (
                <span className={styles.badgeWarning}>
                  {t("interactiveRebase.orphan")}
                </span>
              )}
              {stops && (
                <span
                  className={styles.badgeWarning}
                  title={shown?.stopsAt?.files.join("\n")}
                >
                  ⚠ {t("interactiveRebase.stopsHere")}
                </span>
              )}
              {empty.has(row.oid) && (
                <span className={styles.badge}>
                  {t("interactiveRebase.emptyHere")}
                </span>
              )}
              {commit.published && (
                <span
                  className={styles.badgePublished}
                  title={t("interactiveRebase.publishedHint")}
                >
                  ● {t("interactiveRebase.published")}
                </span>
              )}
              <span
                className={styles.meta}
                title={absoluteTime(commit.authorTime, i18n.language)}
              >
                {commit.author} ·{" "}
                {relativeTime(commit.authorTime, now, i18n.language)}
              </span>
            </div>
          );
        })}
      </div>
      <div className={styles.editor}>
        {editor ? (
          <div className={styles.editorLabel}>
            <label htmlFor={editorId}>{editor.label}</label>
            <textarea
              id={editorId}
              className={styles.message}
              data-message="true"
              value={editor.value}
              rows={4}
              spellCheck={false}
              onChange={(event) =>
                selected && store().setMessage(selected, event.target.value)
              }
            />
            {editor.note && <p className={styles.note}>{editor.note}</p>}
          </div>
        ) : (
          <p className={styles.note}>{t("interactiveRebase.messageHint")}</p>
        )}
      </div>
      <Prediction view={view} />
      <footer className={styles.footer}>
        <button
          type="button"
          className={styles.button}
          disabled={original || busy}
          onClick={() => store().resetPlan()}
        >
          {t("interactiveRebase.resetPlan")}
        </button>
        <button
          type="button"
          className={styles.button}
          disabled={busy}
          onClick={() => store().close()}
        >
          {t("interactiveRebase.cancel")}
        </button>
        <button
          type="button"
          className={styles.primary}
          disabled={!canApply}
          onClick={() => store().requestApply()}
        >
          {busy
            ? t("interactiveRebase.applying")
            : t("interactiveRebase.apply")}
        </button>
      </footer>
      {view.confirming && prediction && (
        <ApplyDialog view={view} prediction={prediction} />
      )}
    </>
  );
}

function Prediction({ view }: { view: RebaseView }) {
  const { t } = useTranslation();
  const reload = useInteractiveRebaseStore((s) => s.reload);
  const preview = view.preview!;
  const prediction = view.predicting ? null : view.prediction;
  const short = (oid: string) =>
    preview.commits.find((c) => c.commit.oid === oid)?.commit.short ?? "";

  const body = (() => {
    if (view.error) {
      return (
        <ErrorReport error={view.error} lead={t("interactiveRebase.failed")} />
      );
    }
    if (!prediction) {
      return <p>{t("interactiveRebase.predicting")}</p>;
    }
    const problem = prediction.problem;
    if (problem) {
      return (
        <p className={styles.problem}>
          {t(`interactiveRebase.problem.${problem.kind}`, {
            short: problem.kind === "emptyMessage" ? short(problem.oid) : "",
            version: preview.gitVersion,
          })}
          {problem.kind === "changed" && (
            <>
              {" "}
              <button
                type="button"
                className={styles.button}
                onClick={() => void reload()}
              >
                {t("interactiveRebase.reload")}
              </button>
            </>
          )}
        </p>
      );
    }
    const summary = [
      t("interactiveRebase.after", { count: prediction.commitsAfter }),
      prediction.dropped.length > 0 &&
        t("interactiveRebase.droppedCount", {
          count: prediction.dropped.length,
        }),
      prediction.empty.length > 0 &&
        t("interactiveRebase.emptyCount", { count: prediction.empty.length }),
    ]
      .filter(Boolean)
      .join(" · ");
    return (
      <>
        {prediction.stopsAt && (
          <div className={styles.stop}>
            <p>
              ⚠{" "}
              {t("interactiveRebase.stops", {
                step: prediction.stopsAt.step,
                of: prediction.stopsAt.of,
                short: prediction.stopsAt.commit.short,
              })}
            </p>
            <ul className={styles.files}>
              {prediction.stopsAt.files.map((file) => (
                <li key={file} className="selectable">
                  {file}
                </li>
              ))}
            </ul>
          </div>
        )}
        {!prediction.predicted && (
          <p className={styles.note}>
            {t("interactiveRebase.notPredicted", {
              version: preview.gitVersion,
            })}
          </p>
        )}
        <p>{summary}</p>
        {prediction.publishedIn.length > 0 && (
          <p className={styles.warning}>
            {t("interactiveRebase.publishedRewritten", {
              remotes: prediction.publishedIn.join(", "),
            })}
          </p>
        )}
      </>
    );
  })();

  return (
    <div
      className={styles.prediction}
      role="status"
      aria-label={t("interactiveRebase.predictionLabel")}
    >
      {view.changed && (
        <p className={styles.note}>
          {t(
            view.changed === "kept"
              ? "interactiveRebase.changedKept"
              : "interactiveRebase.changedReset",
          )}
        </p>
      )}
      {body}
    </div>
  );
}

function ApplyDialog({
  view,
  prediction,
}: {
  view: RebaseView;
  prediction: PlanPreview;
}) {
  const { t } = useTranslation();
  const store = useInteractiveRebaseStore.getState;
  const preview = view.preview!;
  const branch = preview.branch ?? "";
  const save = preview.blocked !== null;
  const conflicts = prediction.stopsAt !== null;
  const stop = prediction.stopsAt;
  const confirmLabel = t(
    `interactiveRebase.confirm.${
      save
        ? conflicts
          ? "saveAndActWithConflicts"
          : "saveAndAct"
        : conflicts
          ? "actWithConflicts"
          : "act"
    }`,
  );
  return (
    <ConfirmDialog
      title={t("interactiveRebase.confirm.title", { branch })}
      confirmLabel={confirmLabel}
      cancelLabel={t("interactiveRebase.cancel")}
      danger={
        prediction.dropped.length > 0 || prediction.publishedIn.length > 0
      }
      onConfirm={() => void store().apply()}
      onCancel={() => store().cancelApply()}
    >
      <p>
        {t("interactiveRebase.confirm.rewritten", {
          count: prediction.rewritten,
          after: prediction.commitsAfter,
        })}
      </p>
      {prediction.dropped.length > 0 && (
        <>
          <p>
            {t("interactiveRebase.confirm.dropped", {
              count: prediction.dropped.length,
            })}
          </p>
          <ul className={styles.files}>
            {prediction.dropped.map((c) => (
              <li key={c.oid}>
                <code>{c.short}</code> {c.subject}
              </li>
            ))}
          </ul>
          <p>{t("interactiveRebase.confirm.recover", { branch })}</p>
        </>
      )}
      {prediction.publishedIn.length > 0 && (
        <p>
          {t("interactiveRebase.confirm.published", {
            branch,
            remotes: prediction.publishedIn.join(", "),
          })}
        </p>
      )}
      {stop && (
        <>
          <p>
            {t("interactiveRebase.confirm.stops", {
              step: stop.step,
              of: stop.of,
              short: stop.commit.short,
              subject: stop.commit.subject,
            })}
          </p>
          <ul className={styles.files}>
            {stop.files.map((file) => (
              <li key={file}>{file}</li>
            ))}
          </ul>
          <p>{t("interactiveRebase.confirm.stopsThen")}</p>
        </>
      )}
      {preview.blocked && (
        <>
          <p>{t(`rebase.blocked.${preview.blocked}`)}</p>
          <ul className={styles.files}>
            {preview.blocking.map((file) => (
              <li key={file.path}>{file.path}</li>
            ))}
          </ul>
          <p>{t("interactiveRebase.confirm.save", { branch })}</p>
        </>
      )}
    </ConfirmDialog>
  );
}
