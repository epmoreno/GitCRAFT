import { useEffect, useState, type ReactNode } from "react";
import { isShortcut } from "../../app/shortcuts";
import { ContextMenu } from "../../ui/ContextMenu";
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
import { ErrorReport } from "../../ui/ErrorReport";

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
  action,
  withHistory = false,
}: {
  repoId: number;
  oid: string;
  isMerge: boolean;
  /** The untracked files saved in a stash entry (all additions). */
  untracked?: boolean;
  /** An extra button per file (e.g. "Recover"). */
  action?: (file: FileChange) => ReactNode;
  /** Each file's menu offers its history (a commit's files, task 12.4). */
  withHistory?: boolean;
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
    return <ErrorReport error={current.error} lead={t("files.failed")} />;
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
            <FileRow
              key={file.path}
              repoId={repoId}
              oid={oid}
              file={file}
              action={action?.(file)}
              withHistory={withHistory}
            />
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
  action,
  withHistory,
}: {
  repoId: number;
  oid: string;
  file: FileChange;
  action?: ReactNode;
  withHistory: boolean;
}) {
  const { t } = useTranslation();
  const open = useGraphStore((s) => s.openFileDiff);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const showDiff = () =>
    open(repoId, {
      kind: "commit",
      oid,
      path: file.path,
      oldPath: file.oldPath,
    });
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
    <li className={action ? styles.withAction : undefined}>
      <button
        type="button"
        className={styles.file}
        title={t("files.openDiff", { path: file.path })}
        aria-current={isOpen || undefined}
        aria-haspopup={withHistory ? "menu" : undefined}
        onClick={showDiff}
        onContextMenu={(event) => {
          if (!withHistory) return;
          event.preventDefault();
          setMenu({ x: event.clientX, y: event.clientY });
        }}
        onKeyDown={(event) => {
          // The menu key or ⇧F10: the file's menu, under it.
          if (!withHistory || !isShortcut("commitMenu", event)) return;
          event.preventDefault();
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu({ x: rect.left + 24, y: rect.bottom });
        }}
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
      {action}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t("files.menu", { path: file.path })}
          items={[
            { label: t("files.showDiff"), onSelect: showDiff },
            {
              label: t("files.history"),
              onSelect: () =>
                open(repoId, {
                  kind: "history",
                  path: file.path,
                  rev: oid,
                  chosen: null,
                }),
            },
          ]}
          onClose={() => setMenu(null)}
        />
      )}
    </li>
  );
}
