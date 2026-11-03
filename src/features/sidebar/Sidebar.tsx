import { isShortcut } from "../../app/shortcuts";
import { useRef, useState, type MouseEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { GitRef, RepoRefs, StashEntry } from "../../ipc";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { useBranchStore } from "../branches/store";
import { selectedStash, stashSelection, useGraphStore } from "../graph/store";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { useRemoteStore } from "../remote/store";
import { useRememberedScroll, useViewStore } from "../repo/viewState";
import { arrows, trackingDescription } from "../remote/tracking";
import { useStashStore } from "../stash/store";
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
  const scroller = useRef<HTMLElement>(null);
  useRememberedScroll(scroller, repoId === null ? null : `${repoId}:sidebar`);
  const filter = useViewStore((s) =>
    repoId === null ? "" : (s.filters[repoId] ?? ""),
  );
  const setFilter = useViewStore((s) => s.setFilter);
  const goTo = (oid: string) => {
    if (repoId !== null) select(repoId, oid);
  };
  const props = {
    refs,
    repoId,
    filter: filter.trim().toLowerCase(),
    onSelect: goTo,
    onMenu: (event: MouseEvent, label: string, items: MenuItem[]) => {
      event.preventDefault();
      setMenu({ x: event.clientX, y: event.clientY, label, items });
    },
  };

  return (
    <aside
      ref={scroller}
      className={styles.sidebar}
      aria-label={t("zones.sidebar")}
    >
      {repoId !== null && (
        <div className={styles.filter}>
          <input
            type="search"
            className={styles.filterInput}
            value={filter}
            placeholder={t("sidebar.filterPlaceholder")}
            aria-label={t("sidebar.filterLabel")}
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
            onChange={(event) => setFilter(repoId, event.target.value)}
            onKeyDown={(event) => {
              // Esc clears the filter (and does nothing else, e.g. close a diff).
              if (isShortcut("escape", event) && filter !== "") {
                event.nativeEvent.stopPropagation();
                setFilter(repoId, "");
              }
            }}
          />
        </div>
      )}
      <LocalBranches {...props} />
      <Remotes {...props} />
      <Tags {...props} />
      <Stashes {...props} />
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
  /** Lower-cased filter text ("" shows everything). */
  filter: string;
  onSelect: (oid: string) => void;
  onMenu: (event: MouseEvent, label: string, items: MenuItem[]) => void;
};

/** Whether any of `texts` contains the (lower-cased) filter. */
function matches(filter: string, ...texts: (string | null | undefined)[]) {
  return (
    filter === "" || texts.some((text) => text?.toLowerCase().includes(filter))
  );
}

/** "3" or, while filtering, "1/3". */
function shownCount(shown: number, total: number, filter: string) {
  return filter === "" ? String(total) : `${shown}/${total}`;
}

function LocalBranches({ refs, repoId, filter, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const branch = useBranchStore();
  const all = refs?.refs.filter((r) => r.kind === "local") ?? [];
  const branches = all.filter((r) => matches(filter, r.shortName, r.upstream));
  const detached =
    refs?.head.type === "detached" &&
    matches(filter, t("sidebar.detachedHead"), "HEAD")
      ? refs.head
      : null;
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
      repoId={repoId}
      id="local"
      title={t("sidebar.local")}
      count={refs ? shownCount(branches.length, all.length, filter) : null}
      filtering={filter !== ""}
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
              hint={
                ref.upstream
                  ? [
                      ref.upstream,
                      ref.upstreamGone ? t("tracking.goneShort") : arrows(ref),
                    ]
                      .filter(Boolean)
                      .join(" ")
                  : undefined
              }
              title={[
                ref.isHead
                  ? t("sidebar.currentBranch", { name: ref.shortName })
                  : ref.shortName,
                trackingDescription(ref, t),
              ].join("\n")}
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
            <li className={styles.empty}>
              {all.length > 0 && filter !== ""
                ? t("sidebar.noMatches")
                : t("sidebar.noBranches")}
            </li>
          )}
        </ul>
      )}
    </Section>
  );
}

function Remotes({ refs, repoId, filter, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const branch = useBranchStore();
  const fetch = useRemoteStore((s) => s.fetch);
  const fetching = useRemoteStore((s) => s.running !== null);
  const checkoutRemote = (ref: GitRef) => {
    if (repoId !== null && refs) void branch.checkoutRemote(repoId, ref, refs);
  };
  const allRemotes = refs?.remotes ?? [];
  const all = refs?.refs.filter((r) => r.kind === "remote") ?? [];
  // A remote whose name matches shows all its branches.
  const byRemote = (remote: string): GitRef[] =>
    all.filter(
      (r) =>
        r.remote === remote &&
        (matches(filter, remote) || matches(filter, r.shortName)),
    );
  const remotes = allRemotes.filter(
    (remote) => matches(filter, remote) || byRemote(remote).length > 0,
  );
  return (
    <Section
      repoId={repoId}
      id="remote"
      title={t("sidebar.remote")}
      count={
        refs ? shownCount(remotes.length, allRemotes.length, filter) : null
      }
      filtering={filter !== ""}
    >
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : remotes.length === 0 ? (
        <p className={styles.empty}>
          {allRemotes.length > 0 && filter !== ""
            ? t("sidebar.noMatches")
            : t("sidebar.noRemotes")}
        </p>
      ) : (
        remotes.map((remote) => (
          <div key={remote} className={styles.group}>
            <h3
              className={styles.groupTitle}
              onContextMenu={(event) => {
                if (repoId === null) return;
                onMenu(event, remote, [
                  {
                    label: t("remote.fetchOne", { remote }),
                    onSelect: () => void fetch(repoId, remote, false),
                    disabled: fetching,
                    hint: t("remote.busy"),
                  },
                  {
                    label: t("remote.fetchOnePrune", { remote }),
                    onSelect: () => void fetch(repoId, remote, true),
                    disabled: fetching,
                    hint: t("remote.busy"),
                  },
                ]);
              }}
            >
              {remote}
            </h3>
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

function Tags({ refs, repoId, filter, onSelect, onMenu }: ListProps) {
  const { t } = useTranslation();
  const requestCreate = useBranchStore((s) => s.requestCreate);
  // Newest-looking versions first: v10 before v9.
  const all = (refs?.refs.filter((r) => r.kind === "tag") ?? []).sort((a, b) =>
    b.shortName.localeCompare(a.shortName, undefined, { numeric: true }),
  );
  const tags = all.filter((r) => matches(filter, r.shortName));
  return (
    <Section
      repoId={repoId}
      id="tags"
      title={t("sidebar.tags")}
      count={refs ? shownCount(tags.length, all.length, filter) : null}
      filtering={filter !== ""}
    >
      {!refs ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : tags.length === 0 ? (
        <p className={styles.empty}>
          {all.length > 0 && filter !== ""
            ? t("sidebar.noMatches")
            : t("sidebar.noTags")}
        </p>
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

function Stashes({ repoId, filter, onMenu }: ListProps) {
  const { t, i18n } = useTranslation();
  const all = useStashStore((s) =>
    repoId === null ? null : (s.stashes[repoId]?.entries ?? null),
  );
  const entries =
    all?.filter((e) => matches(filter, e.message, e.branch)) ?? null;
  const apply = useStashStore((s) => s.apply);
  const select = useGraphStore((s) => s.select);
  const selected = useGraphStore((s) =>
    repoId === null
      ? null
      : selectedStash(s.graphs[repoId]?.selectedOid ?? null),
  );
  const requestDrop = useStashStore((s) => s.requestDrop);
  const now = useStashStore((s) =>
    repoId === null ? 0 : (s.stashes[repoId]?.readAt ?? 0),
  );

  const menuFor = (repoId: number, entry: StashEntry): MenuItem[] => [
    {
      label: t("stash.apply"),
      onSelect: () =>
        void apply(repoId, entry, { pop: false, saveLocal: false }),
    },
    {
      label: t("stash.pop"),
      onSelect: () =>
        void apply(repoId, entry, { pop: true, saveLocal: false }),
    },
    {
      label: t("stash.dropMenu"),
      onSelect: () => requestDrop(repoId, entry),
      danger: true,
    },
  ];

  return (
    <Section
      repoId={repoId}
      id="stashes"
      title={t("sidebar.stashes")}
      count={
        all && entries ? shownCount(entries.length, all.length, filter) : null
      }
      filtering={filter !== ""}
    >
      {repoId === null || !all || !entries ? (
        <p className={styles.empty}>{t("sidebar.empty")}</p>
      ) : entries.length === 0 ? (
        <p className={styles.empty}>
          {all.length > 0 && filter !== ""
            ? t("sidebar.noMatches")
            : t("sidebar.noStashes")}
        </p>
      ) : (
        <ul className={styles.list}>
          {entries.map((entry) => {
            const where = entry.branch ?? t("sidebar.detachedHead");
            const when = relativeTime(entry.time, now, i18n.language);
            return (
              <Item
                key={entry.oid}
                label={entry.message}
                hint={`${where} · ${when}`}
                badge={
                  entry.origin === "user"
                    ? undefined
                    : t(`stash.origin.${entry.origin}`)
                }
                title={t("stash.tooltip", {
                  message: entry.message,
                  branch: where,
                  date: absoluteTime(entry.time, i18n.language),
                  selector: `stash@{${entry.index}}`,
                })}
                stacked
                selected={entry.oid === selected}
                onClick={() => select(repoId, stashSelection(entry.oid))}
                onContextMenu={(event) =>
                  onMenu(event, entry.message, menuFor(repoId, entry))
                }
              />
            );
          })}
        </ul>
      )}
    </Section>
  );
}

function Section({
  repoId,
  id,
  title,
  count,
  filtering = false,
  action,
  children,
}: {
  repoId: number | null;
  /** Remembers the section's collapsed state per tab. */
  id: string;
  title: string;
  count: string | null;
  /** While filtering every section is open, so matches are never hidden. */
  filtering?: boolean;
  /** A button next to the title (e.g. "new branch"). */
  action?: ReactNode;
  children: ReactNode;
}) {
  const key = `${repoId ?? "none"}:${id}`;
  const open = useViewStore((s) => filtering || !s.collapsed[key]);
  const toggle = useViewStore((s) => s.toggleSection);
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
          disabled={filtering}
          onClick={() => toggle(key)}
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
  badge,
  title,
  stacked = false,
  selected = false,
  onClick,
  onDoubleClick,
  onContextMenu,
}: {
  label: string;
  current?: boolean;
  hint?: string;
  /** Marks entries GitCRAFT made itself. */
  badge?: string;
  title?: string;
  /** Hint on its own line, under the label. */
  stacked?: boolean;
  /** Shown in the detail panel. */
  selected?: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  onContextMenu?: (event: MouseEvent) => void;
}) {
  const { t } = useTranslation();
  return (
    <li>
      <button
        type="button"
        className={[
          styles.item,
          current && styles.current,
          stacked && styles.stacked,
          badge && styles.ours,
          selected && styles.selected,
        ]
          .filter(Boolean)
          .join(" ")}
        aria-current={current || undefined}
        aria-pressed={stacked ? selected : undefined}
        title={
          title ??
          (current ? t("sidebar.currentBranch", { name: label }) : label)
        }
        onClick={onClick}
        onDoubleClick={onDoubleClick}
        onContextMenu={onContextMenu}
      >
        <span className={styles.label}>
          {badge && <span className={styles.badge}>{badge}</span>}
          {label}
        </span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </button>
    </li>
  );
}
