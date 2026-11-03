import { useMemo, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { StatusEntry, WorktreeArea } from "../../ipc";
import { Chevron } from "../../ui/Chevron";
import { useGraphStore } from "../graph/store";
import { buildTree, visibleRows, type DirNode } from "./fileTree";
import { StatusLetter } from "./StatusLetter";
import { useWorkdirStore } from "./store";
import { useWorkdirViewPrefs } from "./viewPrefs";
import styles from "./workdir.module.css";

export type ChangeSection = "unstaged" | "staged";

/** Indentation per tree level, in px. */
const INDENT = 14;

/** Key of a collapsed folder in the view prefs. */
export const dirKey = (repoId: number, section: ChangeSection, path: string) =>
  `${repoId}:${section}:${path}`;

/**
 * The files of one section, as a flat list of paths or as a folder tree
 * (the view chosen in the panel header).
 */
export function ChangeList({
  repoId,
  section,
  label,
  entries,
  disabled,
}: {
  repoId: number;
  section: ChangeSection;
  label: string;
  entries: StatusEntry[];
  disabled: boolean;
}) {
  const layout = useWorkdirViewPrefs((s) => s.layout);
  const row = useFileRowProps(repoId, section, disabled);
  if (layout === "list") {
    return (
      <ul className={styles.list}>
        {entries.map((e) => (
          <FileRow key={e.path} {...row(e)} />
        ))}
      </ul>
    );
  }
  return (
    <FileTree
      repoId={repoId}
      section={section}
      label={label}
      entries={entries}
      row={row}
    />
  );
}

type FileRowProps = {
  repoId: number;
  area: WorktreeArea;
  entry: StatusEntry;
  actionLabel: string;
  actionTitle: string;
  disabled: boolean;
  onAction: () => void;
  /** Unstaged and untracked files only. */
  onDiscard?: () => void;
};

/** What each file row of `section` shows and does. */
function useFileRowProps(
  repoId: number,
  section: ChangeSection,
  disabled: boolean,
): (entry: StatusEntry) => FileRowProps {
  const { t } = useTranslation();
  const stage = useWorkdirStore((s) => s.stage);
  const unstage = useWorkdirStore((s) => s.unstage);
  const requestDiscard = useWorkdirStore((s) => s.requestDiscard);
  return (entry) =>
    section === "unstaged"
      ? {
          repoId,
          entry,
          disabled,
          area: entry.status === "untracked" ? "untracked" : "unstaged",
          actionLabel: t("workdir.stage"),
          actionTitle: t("workdir.stageFile", { path: entry.path }),
          onAction: () => void stage(repoId, [entry.path]),
          onDiscard: () =>
            void requestDiscard(repoId, { kind: "files", paths: [entry.path] }),
        }
      : {
          repoId,
          entry,
          disabled,
          area: "staged",
          actionLabel: t("workdir.unstage"),
          actionTitle: t("workdir.unstageFile", { path: entry.path }),
          // A staged rename lives under both names in the index.
          onAction: () =>
            void unstage(
              repoId,
              entry.oldPath ? [entry.path, entry.oldPath] : [entry.path],
            ),
        };
}

/**
 * Folder tree with the WAI-ARIA tree keyboard model: ↑/↓ move, → opens a
 * folder (or enters it), ← closes it (or goes to its parent), Home/End,
 * Enter opens a file's diff or toggles a folder, Space stages or unstages
 * the file. One row is in the tab order at a time (roving tabindex).
 */
function FileTree({
  repoId,
  section,
  label,
  entries,
  row,
}: {
  repoId: number;
  section: ChangeSection;
  label: string;
  entries: StatusEntry[];
  row: (entry: StatusEntry) => FileRowProps;
}) {
  const collapsed = useWorkdirViewPrefs((s) => s.collapsedDirs);
  const toggleDir = useWorkdirViewPrefs((s) => s.toggleDir);
  const tree = useMemo(() => buildTree(entries), [entries]);
  const rows = visibleRows(
    tree,
    (path) => !!collapsed[dirKey(repoId, section, path)],
  );
  const [active, setActive] = useState<string | null>(null);
  const keyOf = (node: (typeof rows)[number]["node"]) =>
    node.kind === "dir" ? `d:${node.path}` : `f:${node.entry.path}`;
  const activeKey = rows.some((r) => keyOf(r.node) === active)
    ? active
    : rows[0] && keyOf(rows[0].node);

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const target = event.target as HTMLElement;
    if (target.getAttribute("role") !== "treeitem") return;
    const items = [
      ...event.currentTarget.querySelectorAll<HTMLElement>('[role="treeitem"]'),
    ];
    const index = items.indexOf(target);
    const dirPath = target.dataset.dir;
    const expanded = target.getAttribute("aria-expanded") === "true";
    const focus = (item: HTMLElement | undefined) => item?.focus();
    switch (event.key) {
      case "ArrowDown":
        focus(items[index + 1]);
        break;
      case "ArrowUp":
        focus(items[index - 1]);
        break;
      case "Home":
        focus(items[0]);
        break;
      case "End":
        focus(items[items.length - 1]);
        break;
      case "ArrowRight":
        if (dirPath === undefined) return;
        if (expanded) focus(items[index + 1]);
        else toggleDir(dirKey(repoId, section, dirPath));
        break;
      case "ArrowLeft":
        if (dirPath !== undefined && expanded) {
          toggleDir(dirKey(repoId, section, dirPath));
        } else {
          focus(
            items.find((item) => item.dataset.dir === target.dataset.parent),
          );
        }
        break;
      case "Enter":
        if (dirPath !== undefined) toggleDir(dirKey(repoId, section, dirPath));
        else target.querySelector<HTMLElement>("[data-open]")?.click();
        break;
      case " ":
        target.querySelector<HTMLElement>("[data-primary]")?.click();
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <ul
      className={styles.list}
      role="tree"
      aria-label={label}
      onKeyDown={onKeyDown}
    >
      {rows.map(({ node, level, parent }) => {
        const key = keyOf(node);
        const tree = {
          level,
          parent,
          tabIndex: key === activeKey ? 0 : -1,
          onFocus: () => setActive(key),
        };
        return node.kind === "dir" ? (
          <DirRow
            key={key}
            dir={node}
            tree={tree}
            expanded={!collapsed[dirKey(repoId, section, node.path)]}
            onToggle={() => toggleDir(dirKey(repoId, section, node.path))}
          />
        ) : (
          <FileRow
            key={key}
            {...row(node.entry)}
            name={node.name}
            tree={tree}
          />
        );
      })}
    </ul>
  );
}

type TreeItem = {
  level: number;
  parent: string;
  tabIndex: number;
  onFocus: () => void;
};

const treeItemProps = (tree: TreeItem) => ({
  role: "treeitem",
  "aria-level": tree.level,
  tabIndex: tree.tabIndex,
  "data-parent": tree.parent,
  onFocus: (event: React.FocusEvent) => {
    if (event.target === event.currentTarget) tree.onFocus();
  },
  style: {
    paddingLeft: `calc(var(--space-4) + ${(tree.level - 1) * INDENT}px)`,
  },
});

function DirRow({
  dir,
  tree,
  expanded,
  onToggle,
}: {
  dir: DirNode;
  tree: TreeItem;
  expanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li
      className={`${styles.file} ${styles.dirRow}`}
      {...treeItemProps(tree)}
      aria-expanded={expanded}
      data-dir={dir.path}
      title={dir.path}
      onClick={onToggle}
    >
      <Chevron direction={expanded ? "down" : "right"} />
      <span className={styles.dirName}>{dir.name}</span>
      <span
        className={styles.dirCount}
        title={t("workdir.folderFiles", { count: dir.fileCount })}
      >
        {dir.fileCount}
      </span>
    </li>
  );
}

export function PathLabel({
  path,
  oldPath,
  name,
}: {
  path: string;
  oldPath?: string | null;
  /** In the tree only the last segment is shown (the folder is above). */
  name?: string;
}) {
  const { t } = useTranslation();
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const slash = trimmed.lastIndexOf("/");
  const dir = name === undefined && slash >= 0 ? path.slice(0, slash + 1) : "";
  return (
    <span className={styles.path} title={path}>
      {dir && <span className={styles.dir}>{dir}</span>}
      <span>{name ?? path.slice(slash + 1)}</span>
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
  name,
  tree,
}: FileRowProps & { name?: string; tree?: TreeItem }) {
  const { t } = useTranslation();
  const open = useGraphStore((s) => s.openFileDiff);
  const isOpen = useGraphStore((s) => {
    const f = s.graphs[repoId]?.openFile;
    return f?.kind === "worktree" && f.area === area && f.path === entry.path;
  });
  // An untracked directory has no single-file diff.
  const hasDiff = !entry.path.endsWith("/");
  // In the tree the row itself takes the keyboard focus, not its buttons.
  const inner = tree ? -1 : undefined;
  return (
    <li
      className={isOpen ? `${styles.file} ${styles.fileOpen}` : styles.file}
      {...(tree && { ...treeItemProps(tree), "aria-selected": isOpen })}
      onDoubleClick={disabled ? undefined : onAction}
    >
      <StatusLetter status={entry.status} />
      {hasDiff ? (
        <button
          type="button"
          className={styles.pathButton}
          aria-label={t("workdir.viewChanges", { path: entry.path })}
          aria-current={isOpen || undefined}
          tabIndex={inner}
          data-open
          onClick={() =>
            open(repoId, {
              kind: "worktree",
              area,
              path: entry.path,
              oldPath: entry.oldPath,
            })
          }
        >
          <PathLabel path={entry.path} oldPath={entry.oldPath} name={name} />
        </button>
      ) : (
        <PathLabel path={entry.path} oldPath={entry.oldPath} name={name} />
      )}
      {onDiscard && (
        <button
          type="button"
          className={`${styles.rowAction} ${styles.danger}`}
          aria-label={t("workdir.discardFile", { path: entry.path })}
          title={t("workdir.discardFile", { path: entry.path })}
          disabled={disabled}
          tabIndex={inner}
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
        tabIndex={inner}
        data-primary
        onClick={onAction}
      >
        {actionLabel}
      </button>
    </li>
  );
}
