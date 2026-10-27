import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { GitRef, RepoRefs } from "../../ipc";
import { useGraphStore } from "../graph/store";
import styles from "./sidebar.module.css";

type Props = { repoId: number | null };

/** Branches, remotes, tags and stashes of the active repository. */
export function Sidebar({ repoId }: Props) {
  const { t } = useTranslation();
  const refs = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs ?? null),
  );
  const select = useGraphStore((s) => s.select);
  const goTo = (oid: string) => {
    if (repoId !== null) select(repoId, oid);
  };

  return (
    <aside className={styles.sidebar} aria-label={t("zones.sidebar")}>
      <LocalBranches refs={refs} onSelect={goTo} />
      <Remotes refs={refs} onSelect={goTo} />
      <Tags refs={refs} onSelect={goTo} />
      <Section title={t("sidebar.stashes")} count={null}>
        {/* Stashes are not read yet (roadmap 7.x). */}
        <p className={styles.empty}>
          {repoId === null ? t("sidebar.empty") : t("sidebar.comingSoon")}
        </p>
      </Section>
    </aside>
  );
}

type ListProps = { refs: RepoRefs | null; onSelect: (oid: string) => void };

function LocalBranches({ refs, onSelect }: ListProps) {
  const { t } = useTranslation();
  const branches = refs?.refs.filter((r) => r.kind === "local") ?? [];
  const detached = refs?.head.type === "detached" ? refs.head : null;
  return (
    <Section title={t("sidebar.local")} count={refs ? branches.length : null}>
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : (
        <ul className={styles.list}>
          {detached && (
            <Item
              label={t("sidebar.detachedHead")}
              current
              onClick={() => onSelect(detached.oid)}
            />
          )}
          {branches.map((ref) => (
            <Item
              key={ref.name}
              label={ref.shortName}
              current={ref.isHead}
              hint={ref.upstream ?? undefined}
              onClick={() => onSelect(ref.target)}
            />
          ))}
          {branches.length === 0 && !detached && (
            <li className={styles.empty}>{t("sidebar.noBranches")}</li>
          )}
        </ul>
      )}
    </Section>
  );
}

function Remotes({ refs, onSelect }: ListProps) {
  const { t } = useTranslation();
  const remotes = refs?.remotes ?? [];
  const byRemote = (remote: string): GitRef[] =>
    refs?.refs.filter((r) => r.kind === "remote" && r.remote === remote) ?? [];
  return (
    <Section title={t("sidebar.remote")} count={refs ? remotes.length : null}>
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : remotes.length === 0 ? (
        <p className={styles.empty}>{t("sidebar.noRemotes")}</p>
      ) : (
        remotes.map((remote) => (
          <div key={remote} className={styles.group}>
            <h3 className={styles.groupTitle}>{remote}</h3>
            <ul className={styles.list}>
              {byRemote(remote).map((ref) => (
                <Item
                  key={ref.name}
                  // Within its group the remote prefix is redundant.
                  label={ref.shortName.slice(remote.length + 1)}
                  onClick={() => onSelect(ref.target)}
                />
              ))}
            </ul>
          </div>
        ))
      )}
    </Section>
  );
}

function Tags({ refs, onSelect }: ListProps) {
  const { t } = useTranslation();
  // Newest-looking versions first: v10 before v9.
  const tags = (refs?.refs.filter((r) => r.kind === "tag") ?? []).sort((a, b) =>
    b.shortName.localeCompare(a.shortName, undefined, { numeric: true }),
  );
  return (
    <Section title={t("sidebar.tags")} count={refs ? tags.length : null}>
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : tags.length === 0 ? (
        <p className={styles.empty}>{t("sidebar.noTags")}</p>
      ) : (
        <ul className={styles.list}>
          {tags.map((ref) => (
            <Item
              key={ref.name}
              label={ref.shortName}
              onClick={() => onSelect(ref.target)}
            />
          ))}
        </ul>
      )}
    </Section>
  );
}

function Section({
  title,
  count,
  children,
}: {
  title: string;
  count: number | null;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <section className={styles.section}>
      <h2 className={styles.title}>
        <button
          type="button"
          className={styles.toggle}
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          <span className={styles.chevron} aria-hidden="true">
            {open ? "▾" : "▸"}
          </span>
          {title}
          {count !== null && <span className={styles.count}>{count}</span>}
        </button>
      </h2>
      {open && children}
    </section>
  );
}

function Item({
  label,
  current = false,
  hint,
  onClick,
}: {
  label: string;
  current?: boolean;
  hint?: string;
  onClick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li>
      <button
        type="button"
        className={current ? `${styles.item} ${styles.current}` : styles.item}
        aria-current={current || undefined}
        title={current ? t("sidebar.currentBranch", { name: label }) : label}
        onClick={onClick}
      >
        <span className={styles.label}>{label}</span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </button>
    </li>
  );
}
