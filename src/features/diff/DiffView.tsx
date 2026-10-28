import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type FileDiff,
  type HunkSelection,
} from "../../ipc";
import { useGraphStore, type OpenFile } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";
import styles from "./diff.module.css";

type Loaded =
  | { key: string; diff: FileDiff; error?: undefined }
  | { key: string; error: CommandError; diff?: undefined };

/** Selected changed lines: hunk index → line indexes. */
type Selection = Map<number, Set<number>>;

const SIGN = { context: " ", added: "+", removed: "−" } as const;

function load(repoId: number, file: OpenFile, full: boolean) {
  return file.kind === "commit"
    ? commands.commitFileDiff(repoId, file.oid, file.path, file.oldPath, full)
    : commands.worktreeFileDiff(
        repoId,
        file.path,
        file.oldPath,
        file.area,
        full,
      );
}

/** The diff of one file — of a commit, or an uncommitted change — shown
 * over the history. Uncommitted changes can be staged/unstaged by hunk or
 * by line from here. */
export function DiffView({ repoId, file }: { repoId: number; file: OpenFile }) {
  const { t } = useTranslation();
  const close = useGraphStore((s) => s.closeFile);
  // Uncommitted diffs follow the status: re-read when it changes.
  const version = useWorkdirStore((s) =>
    file.kind === "worktree" ? (s.statuses[repoId]?.version ?? 0) : 0,
  );
  const [full, setFull] = useState(false);
  const key = JSON.stringify({ file, full, version });
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    let stale = false;
    const { file, full } = JSON.parse(key) as { file: OpenFile; full: boolean };
    void load(repoId, file, full).then((result) => {
      if (stale) return;
      setLoaded(
        result.status === "ok"
          ? { key, diff: result.data }
          : { key, error: result.error },
      );
    });
    return () => {
      stale = true;
    };
  }, [repoId, key]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close(repoId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId, close]);

  // While a refreshed diff of the same file loads, keep showing the last one.
  const sameFile =
    loaded !== null &&
    JSON.stringify((JSON.parse(loaded.key) as { file: OpenFile }).file) ===
      JSON.stringify(file);
  const current = loaded?.key === key || sameFile ? loaded : null;

  return (
    <section
      className={styles.view}
      aria-label={t("diff.label", { path: file.path })}
    >
      <header className={styles.header}>
        <button
          type="button"
          className={styles.back}
          title={t("diff.backHint")}
          onClick={() => close(repoId)}
        >
          ← {t("diff.back")}
        </button>
        <span className={`${styles.path} selectable`}>
          {file.oldPath && (
            <span className={styles.oldPath}>{file.oldPath} → </span>
          )}
          {file.path}
        </span>
        {file.kind === "commit" ? (
          <code className={`${styles.sha} selectable`}>
            {file.oid.slice(0, 7)}
          </code>
        ) : (
          <span className={styles.area}>{t(`diff.area.${file.area}`)}</span>
        )}
      </header>

      <div className={styles.body}>
        {!current ? (
          <p className={styles.message}>{t("diff.loading")}</p>
        ) : current.error ? (
          <div className={styles.message} role="alert">
            <p>{t("diff.failed")}</p>
            {current.error.details && (
              <pre className="selectable">{current.error.details}</pre>
            )}
          </div>
        ) : (
          <DiffBody
            // A new diff (other content) starts with an empty selection.
            key={current.diff.fingerprint}
            repoId={repoId}
            file={file}
            diff={current.diff}
            onShowAll={() => setFull(true)}
          />
        )}
      </div>
    </section>
  );
}

function DiffBody({
  repoId,
  file,
  diff,
  onShowAll,
}: {
  repoId: number;
  file: OpenFile;
  diff: FileDiff;
  onShowAll: () => void;
}) {
  const { t } = useTranslation();
  const applySelection = useWorkdirStore((s) => s.applySelection);
  const requestDiscard = useWorkdirStore((s) => s.requestDiscard);
  const busy = useWorkdirStore((s) => s.statuses[repoId]?.busy ?? false);
  const [selection, setSelection] = useState<Selection>(new Map());
  const [anchor, setAnchor] = useState<{ hunk: number; line: number } | null>(
    null,
  );

  const editable = file.kind === "worktree" && !diff.binary && !diff.tooLarge;
  const action =
    file.kind === "worktree" && file.area === "staged" ? "unstage" : "stage";
  const selectedCount = [...selection.values()].reduce((n, s) => n + s.size, 0);
  // Only tracked files' unstaged changes can be discarded by hunk/line
  // (untracked files go to the Trash whole, from the panel).
  const discardable =
    editable && file.kind === "worktree" && file.area === "unstaged";

  const discard = (hunks: HunkSelection[]) => {
    if (file.kind !== "worktree" || hunks.length === 0) return;
    setSelection(new Map());
    void requestDiscard(repoId, {
      kind: "selection",
      path: file.path,
      fingerprint: diff.fingerprint,
      selection: hunks,
    });
  };

  const submit = (hunks: HunkSelection[]) => {
    if (file.kind !== "worktree" || hunks.length === 0) return;
    setSelection(new Map());
    void applySelection(repoId, {
      path: file.path,
      oldPath: file.oldPath,
      area: file.area,
      fingerprint: diff.fingerprint,
      selection: hunks,
      action,
    });
  };

  const toggle = (hunk: number, line: number, range: boolean) => {
    const lines = diff.hunks[hunk]?.lines ?? [];
    const next = new Map(selection);
    const set = new Set(next.get(hunk));
    let indexes = [line];
    if (range && anchor?.hunk === hunk) {
      const from = Math.min(anchor.line, line);
      const to = Math.max(anchor.line, line);
      indexes = [];
      for (let i = from; i <= to; i++) {
        if (lines[i]?.kind !== "context") indexes.push(i);
      }
    }
    const select = range || !set.has(line);
    for (const i of indexes) {
      if (select) set.add(i);
      else set.delete(i);
    }
    if (set.size > 0) next.set(hunk, set);
    else next.delete(hunk);
    setSelection(next);
    setAnchor({ hunk, line });
  };

  const selectedHunks = (): HunkSelection[] =>
    [...selection.entries()].map(([hunk, lines]) => ({
      hunk,
      lines: [...lines].sort((a, b) => a - b),
    }));

  return (
    <>
      {editable && (
        <div className={styles.selectionBar}>
          {selectedCount === 0 ? (
            <span className={styles.selectionHint}>
              {t("diff.selectionHint")}
            </span>
          ) : (
            <>
              <button
                type="button"
                className={styles.primary}
                disabled={busy}
                onClick={() => submit(selectedHunks())}
              >
                {t(
                  action === "stage" ? "diff.stageLines" : "diff.unstageLines",
                  { count: selectedCount },
                )}
              </button>
              {discardable && (
                <button
                  type="button"
                  className={styles.dangerButton}
                  disabled={busy}
                  onClick={() => discard(selectedHunks())}
                >
                  {t("diff.discardLines", { count: selectedCount })}
                </button>
              )}
              <button
                type="button"
                className={styles.showAll}
                onClick={() => setSelection(new Map())}
              >
                {t("diff.clearSelection")}
              </button>
            </>
          )}
        </div>
      )}
      {diff.notes.length > 0 && (
        <ul className={styles.notes}>
          {diff.notes.map((note) => (
            <li key={note} className="selectable">
              {note}
            </li>
          ))}
        </ul>
      )}
      {diff.binary ? (
        <p className={styles.message}>{t("diff.binary")}</p>
      ) : diff.tooLarge ? (
        <div className={styles.message}>
          <p>{t("diff.tooLarge", { count: diff.lineCount })}</p>
          <button type="button" className={styles.showAll} onClick={onShowAll}>
            {t("diff.showAnyway")}
          </button>
        </div>
      ) : diff.hunks.length === 0 ? (
        <p className={styles.message}>{t("diff.noChanges")}</p>
      ) : (
        <table className={styles.table}>
          {diff.hunks.map((hunk, h) => (
            <tbody key={`${hunk.oldStart}:${hunk.newStart}`}>
              <tr className={styles.hunk}>
                <td colSpan={editable ? 5 : 4}>
                  <span className="selectable">
                    @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},
                    {hunk.newLines} @@ {hunk.section}
                  </span>
                  {editable && (
                    <button
                      type="button"
                      className={styles.hunkAction}
                      disabled={busy}
                      onClick={() => submit([{ hunk: h, lines: null }])}
                    >
                      {t(
                        action === "stage"
                          ? "diff.stageHunk"
                          : "diff.unstageHunk",
                      )}
                    </button>
                  )}
                  {discardable && (
                    <button
                      type="button"
                      className={`${styles.hunkAction} ${styles.hunkDanger}`}
                      disabled={busy}
                      onClick={() => discard([{ hunk: h, lines: null }])}
                    >
                      {t("diff.discardHunk")}
                    </button>
                  )}
                </td>
              </tr>
              {hunk.lines.map((line, i) => {
                const changed = line.kind !== "context";
                const selected = selection.get(h)?.has(i) ?? false;
                return (
                  <tr
                    key={i}
                    className={`${styles[line.kind]} ${selected ? styles.selected : ""}`}
                    data-kind={line.kind}
                    onClick={
                      editable && changed
                        ? (e) => toggle(h, i, e.shiftKey)
                        : undefined
                    }
                  >
                    {editable && (
                      <td className={styles.pick}>
                        {changed && (
                          <input
                            type="checkbox"
                            checked={selected}
                            aria-label={t("diff.selectLine", {
                              line: line.newLine ?? line.oldLine,
                            })}
                            onClick={(e) => {
                              e.stopPropagation();
                              toggle(h, i, e.shiftKey);
                            }}
                            onChange={() => {}}
                          />
                        )}
                      </td>
                    )}
                    <td className={styles.number}>{line.oldLine ?? ""}</td>
                    <td className={styles.number}>{line.newLine ?? ""}</td>
                    <td className={styles.sign} aria-hidden="true">
                      {SIGN[line.kind]}
                    </td>
                    <td className={`${styles.code} selectable`}>
                      {line.content}
                      {line.noNewline && (
                        <span
                          className={styles.noNewline}
                          title={t("diff.noNewline")}
                          aria-label={t("diff.noNewline")}
                        >
                          ⏎̸
                        </span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          ))}
        </table>
      )}
    </>
  );
}
