import { useState, type MouseEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { GitRef, RepoRefs } from "../../ipc";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { useBranchStore } from "../branches/store";
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
  const [menu, setMenu] = useState<Menu | null>(null);
  const goTo = (oid: string) => {
    if (repoId !== null) select(repoId, oid);
  };
  const props = {
    refs,
    repoId,
    onSelect: goTo,
    onMenu: (event: MouseEvent, label: string, items: MenuItem[]) => {
      event.preventDefault();
      setMenu({ x: event.clientX, y: event.clientY, label, items });
    },
  };

  return (
    <aside className={styles.sidebar} aria-label={t("zones.sidebar")}>
      <LocalBranches {...props} />
      <Remotes {...props} />
      <Tags {...props} />
      <Section title={t("sidebar.stashes")} count={null}>
        {/* Stashes are not read yet (roadmap 7.x). */}
        <p className={styles.empty}>
          {repoId === null ? t("sidebar.empty") : t("sidebar.comingSoon")}
        </p>
      </Section>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={menu.label}
          items={menu.items}
          onClose={() => setMenu(null)}
        />
      )}
    </aside>
  );
}

type Menu = { x: number; y: number; label: string; items: MenuItem[] };

type ListProps = {
  refs: RepoRefs | null;
  repoId: number | null;
  onSelect: (oid: string) => void;
  onMenu: (event: MouseEvent, label: string, items: MenuItem[]) => void;
};

function LocalBranches({ refs, repoId, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const branch = useBranchStore();
  const branches = refs?.refs.filter((r) => r.kind === "local") ?? [];
  const detached = refs?.head.type === "detached" ? refs.head : null;
  const headOid = refs?.head.oid ?? null;
  const headLabel =
    refs?.head.type === "branch" ? refs.head.name : headOid?.slice(0, 7);

  const menuFor = (ref: GitRef): MenuItem[] =>
    repoId === null
      ? []
      : [
          {
            label: t("branches.checkout", { name: ref.shortName }),
            onSelect: () => void branch.checkout(repoId, ref.shortName),
            disabled: ref.isHead,
            hint: t("branches.alreadyCurrent"),
          },
          {
            label: t("branches.createFromMenu", { name: ref.shortName }),
            onSelect: () =>
              branch.requestCreate(repoId, ref.name, ref.shortName),
          },
          {
            label: t("branches.renameMenu"),
            onSelect: () => branch.requestRename(repoId, ref.shortName),
          },
          {
            label: t("branches.deleteMenu"),
            onSelect: () => void branch.requestDelete(repoId, ref.shortName),
            disabled: ref.isHead,
            hint: t("branches.cannotDeleteCurrent"),
            danger: true,
          },
        ];

  const newBranch =
    repoId !== null && refs ? (
      <button
        type="button"
        className={styles.sectionAction}
        aria-label={t("branches.new")}
        title={headOid ? t("branches.new") : t("branches.needsCommit")}
        disabled={!headOid}
        onClick={() => {
          if (headOid) branch.requestCreate(repoId, headOid, headLabel ?? "");
        }}
      >
        +
      </button>
    ) : null;

  return (
    <Section
      title={t("sidebar.local")}
      count={refs ? branches.length : null}
      action={newBranch}
    >
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : (
        <ul className={styles.list}>
          {detached && (
            <Item
              label={t("sidebar.detachedHead")}
              current
              onClick={() => onSelect(detached.oid)}
              onContextMenu={(event) => {
                if (repoId === null) return;
                onMenu(event, t("sidebar.detachedHead"), [
                  {
                    label: t("branches.createHere"),
                    onSelect: () =>
                      branch.requestCreate(
                        repoId,
                        detached.oid,
                        detached.oid.slice(0, 7),
                      ),
                  },
                ]);
              }}
            />
          )}
          {branches.map((ref) => (
            <Item
              key={ref.name}
              label={ref.shortName}
              current={ref.isHead}
              hint={ref.upstream ?? undefined}
              onClick={() => onSelect(ref.target)}
              onDoubleClick={() => {
                if (repoId !== null && !ref.isHead)
                  void branch.checkout(repoId, ref.shortName);
              }}
              onContextMenu={(event) =>
                onMenu(event, ref.shortName, menuFor(ref))
              }
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

function Remotes({ refs, repoId, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const branch = useBranchStore();
  const checkoutRemote = (ref: GitRef) => {
    if (repoId !== null && refs) void branch.checkoutRemote(repoId, ref, refs);
  };
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
                  onDoubleClick={() => checkoutRemote(ref)}
                  onContextMenu={(event) => {
                    if (repoId === null) return;
                    onMenu(event, ref.shortName, [
                      {
                        label: t("branches.checkoutRemote", {
                          name: ref.shortName,
                        }),
                        onSelect: () => checkoutRemote(ref),
                      },
                      {
                        label: t("branches.createFromMenu", {
                          name: ref.shortName,
                        }),
                        onSelect: () =>
                          branch.requestCreate(
                            repoId,
                            ref.name,
                            ref.shortName,
                            {
                              track: true,
                              name: ref.shortName.slice(remote.length + 1),
                            },
                          ),
                      },
                    ]);
                  }}
                />
              ))}
            </ul>
          </div>
        ))
      )}
    </Section>
  );
}

function Tags({ refs, repoId, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const requestCreate = useBranchStore((s) => s.requestCreate);
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
              onContextMenu={(event) => {
                if (repoId === null) return;
                onMenu(event, ref.shortName, [
                  {
                    label: t("branches.createFromMenu", {
                      name: ref.shortName,
                    }),
                    onSelect: () =>
                      requestCreate(repoId, ref.name, ref.shortName),
                  },
                ]);
              }}
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
  action,
  children,
}: {
  title: string;
  count: number | null;
  /** A button next to the title (e.g. "new branch"). */
  action?: ReactNode;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <section className={styles.section}>
      <h2
        className={
          action ? `${styles.title} ${styles.withAction}` : styles.title
        }
      >
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
        {action}
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
  onDoubleClick,
  onContextMenu,
}: {
  label: string;
  current?: boolean;
  hint?: string;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (event: MouseEvent) => void;
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
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      >
        <span className={styles.label}>{label}</span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </button>
    </li>
  );
}
