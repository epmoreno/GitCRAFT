import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CommandError, type FileDiff } from "../../ipc";
import { useGraphStore, type OpenFile } from "../graph/store";
import styles from "./diff.module.css";

type Request = OpenFile & { full: boolean };

type Loaded =
  | { key: string; diff: FileDiff; error?: undefined }
  | { key: string; error: CommandError; diff?: undefined };

const keyOf = (r: Request) => JSON.stringify(r);

const SIGN = { context: " ", added: "+", removed: "−" } as const;

/** The diff of one file of a commit, shown over the history. */
export function DiffView({ repoId, file }: { repoId: number; file: OpenFile }) {
  const { t } = useTranslation();
  const close = useGraphStore((s) => s.closeFile);
  const [full, setFull] = useState(false);
  const request: Request = { ...file, full };
  const key = keyOf(request);
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    let stale = false;
    const { oid, path, oldPath, full } = JSON.parse(key) as Request;
    void commands
      .commitFileDiff(repoId, oid, path, oldPath, full)
      .then((result) => {
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

  const current = loaded?.key === key ? loaded : null;

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
        <code className={`${styles.sha} selectable`}>
          {file.oid.slice(0, 7)}
        </code>
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
          <DiffBody diff={current.diff} onShowAll={() => setFull(true)} />
        )}
      </div>
    </section>
  );
}

function DiffBody({
  diff,
  onShowAll,
}: {
  diff: FileDiff;
  onShowAll: () => void;
}) {
  const { t } = useTranslation();
  return (
    <>
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
          {diff.hunks.map((hunk) => (
            <tbody key={`${hunk.oldStart}:${hunk.newStart}`}>
              <tr className={styles.hunk}>
                <td colSpan={4} className="selectable">
                  @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},
                  {hunk.newLines} @@ {hunk.section}
                </td>
              </tr>
              {hunk.lines.map((line, i) => (
                <tr key={i} className={styles[line.kind]} data-kind={line.kind}>
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
              ))}
            </tbody>
          ))}
        </table>
      )}
    </>
  );
}
