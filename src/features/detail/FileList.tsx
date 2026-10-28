import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type CommitChanges,
  type FileChange,
  type FileStatus,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import styles from "./detail.module.css";

const STATUS_LETTER: Record<FileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  unmerged: "U",
  untracked: "?",
  unknown: "?",
};

type Loaded =
  | { oid: string; changes: CommitChanges; error?: undefined }
  | { oid: string; error: CommandError; changes?: undefined };

/** Files changed by a commit, with +/− line counts. */
export function FileList({
  repoId,
  oid,
  isMerge,
  untracked = false,
}: {
  repoId: number;
  oid: string;
  isMerge: boolean;
  /** The untracked files saved in a stash entry (all additions). */
  untracked?: boolean;
}) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    let stale = false;
    void commands.commitChanges(repoId, oid).then((result) => {
      if (stale) return;
      setLoaded(
        result.status === "ok"
          ? { oid, changes: result.data }
          : { oid, error: result.error },
      );
    });
    return () => {
      stale = true;
    };
  }, [repoId, oid]);

  const current = loaded?.oid === oid ? loaded : null;
  if (!current) return <p className={styles.muted}>{t("files.loading")}</p>;
  if (current.error) {
    return (
      <div role="alert">
        <p>{t("files.failed")}</p>
        {current.error.details && (
          <pre className={`${styles.raw} selectable`}>
            {current.error.details}
          </pre>
        )}
      </div>
    );
  }

  const { files, base } = current.changes;
  const added = files.reduce((sum, f) => sum + (f.additions ?? 0), 0);
  const removed = files.reduce((sum, f) => sum + (f.deletions ?? 0), 0);

  return (
    <section className={styles.files} aria-label={t("files.title")}>
      <h3 className={styles.filesTitle}>
        {t(untracked ? "files.untrackedCount" : "files.count", {
          count: files.length,
        })}
        <span className={styles.added}>+{added}</span>
        <span className={styles.removed}>−{removed}</span>
      </h3>
      {isMerge && base && (
        <p className={styles.muted}>
          {t("files.againstFirstParent", { sha: base.slice(0, 7) })}
        </p>
      )}
      {files.length === 0 ? (
        <p className={styles.muted}>{t("files.none")}</p>
      ) : (
        <ul className={styles.fileList}>
          {files.map((file) => (
            <FileRow key={file.path} repoId={repoId} oid={oid} file={file} />
          ))}
        </ul>
      )}
    </section>
  );
}

function FileRow({
  repoId,
  oid,
  file,
}: {
  repoId: number;
  oid: string;
  file: FileChange;
}) {
  const { t } = useTranslation();
  const open = useGraphStore((s) => s.openFileDiff);
  const isOpen = useGraphStore((s) => {
    const current = s.graphs[repoId]?.openFile;
    return (
      current?.kind === "commit" &&
      current.oid === oid &&
      current.path === file.path
    );
  });
  const slash = file.path.lastIndexOf("/");
  const dir = slash >= 0 ? file.path.slice(0, slash + 1) : "";
  const name = file.path.slice(slash + 1);
  return (
    <li>
      <button
        type="button"
        className={styles.file}
        title={t("files.openDiff", { path: file.path })}
        aria-current={isOpen || undefined}
        onClick={() =>
          open(repoId, {
            kind: "commit",
            oid,
            path: file.path,
            oldPath: file.oldPath,
          })
        }
      >
        <span
          className={`${styles.status} ${styles[`status-${file.status}`] ?? ""}`}
          title={t(`files.status.${file.status}`)}
          aria-label={t(`files.status.${file.status}`)}
        >
          {STATUS_LETTER[file.status]}
        </span>
        <span className={styles.path}>
          {dir && <span className={styles.dir}>{dir}</span>}
          <span>{name}</span>
          {file.oldPath && (
            <span className={styles.oldPath}>
              {t("files.renamedFrom", { path: file.oldPath })}
            </span>
          )}
        </span>
        {file.additions === null ? (
          <span className={styles.binary}>{t("files.binary")}</span>
        ) : (
          <span className={styles.counts}>
            <span className={styles.added}>+{file.additions}</span>
            <span className={styles.removed}>−{file.deletions}</span>
          </span>
        )}
      </button>
    </li>
  );
}
