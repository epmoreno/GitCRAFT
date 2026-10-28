import { useTranslation } from "react-i18next";
import type { ConflictEntry, StatusEntry } from "../../ipc";
import { useWorkdirStore } from "./store";
import { StatusLetter } from "./StatusLetter";
import styles from "./workdir.module.css";

/** Right-hand panel for the "uncommitted changes" entry. */
export function WorkingDirPanel({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const entry = useWorkdirStore((s) => s.statuses[repoId]);
  const status = entry?.status ?? null;

  if (!status) {
    return entry?.error ? (
      <div role="alert">
        <p>{t("workdir.failed")}</p>
        {entry.error.details && (
          <pre className="selectable">{entry.error.details}</pre>
        )}
      </div>
    ) : (
      <p className={styles.muted}>{t("workdir.loading")}</p>
    );
  }

  const unstaged = [...status.unstaged, ...status.untracked];
  return (
    <div className={styles.panel}>
      <h2 className={styles.title}>{t("workdir.row")}</h2>
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
      <Section title={t("workdir.unstaged")} count={unstaged.length}>
        {unstaged.map((e) => (
          <FileRow key={`u:${e.path}`} entry={e} />
        ))}
      </Section>
      <Section title={t("workdir.staged")} count={status.staged.length}>
        {status.staged.map((e) => (
          <FileRow key={`s:${e.path}`} entry={e} />
        ))}
      </Section>
    </div>
  );
}

function Section({
  title,
  count,
  children,
}: {
  title: string;
  count: number;
  children: React.ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <section className={styles.section} aria-label={title}>
      <h3 className={styles.sectionTitle}>
        {title} <span className={styles.count}>{count}</span>
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

function FileRow({ entry }: { entry: StatusEntry }) {
  return (
    <li className={styles.file}>
      <StatusLetter status={entry.status} />
      <PathLabel path={entry.path} oldPath={entry.oldPath} />
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
