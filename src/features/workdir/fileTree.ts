import type { FileStatus, StatusEntry } from "../../ipc";

export type SortOrder = "name" | "status";

/** Order of the "by status" sort: the usual changes first, then the rest. */
const STATUS_ORDER: FileStatus[] = [
  "modified",
  "added",
  "deleted",
  "renamed",
  "copied",
  "typeChanged",
  "untracked",
  "unmerged",
  "unknown",
];

const collator = new Intl.Collator(undefined, { numeric: true });
export const compareNames = (a: string, b: string) => collator.compare(a, b);

const byStatus = (a: StatusEntry, b: StatusEntry) =>
  STATUS_ORDER.indexOf(a.status) - STATUS_ORDER.indexOf(b.status);

/** The flat list's order: by path, or by status and then path. */
export function sortEntries(
  entries: readonly StatusEntry[],
  sort: SortOrder,
): StatusEntry[] {
  return [...entries].sort(
    (a, b) =>
      (sort === "status" ? byStatus(a, b) : 0) || compareNames(a.path, b.path),
  );
}

export type DirNode = {
  kind: "dir";
  /** Last segment ("ui"). */
  name: string;
  /** Full path without a trailing slash ("src/ui"). */
  path: string;
  children: TreeNode[];
  /** Files anywhere below it. */
  fileCount: number;
};

export type FileNode = {
  kind: "file";
  /** Last segment; an untracked directory keeps its slash ("notes/"). */
  name: string;
  entry: StatusEntry;
};

export type TreeNode = DirNode | FileNode;

/**
 * Groups changed paths into folders. Folders come first, by name; files
 * follow by name or by status. An untracked directory (`notes/`, git does
 * not list its contents) is a leaf, not a folder: it can only be staged or
 * discarded whole.
 */
export function buildTree(
  entries: readonly StatusEntry[],
  sort: SortOrder = "name",
): TreeNode[] {
  const root: DirNode = {
    kind: "dir",
    name: "",
    path: "",
    children: [],
    fileCount: 0,
  };
  for (const entry of entries) {
    const isDir = entry.path.endsWith("/");
    const parts = (isDir ? entry.path.slice(0, -1) : entry.path).split("/");
    const leaf = parts.pop() ?? entry.path;
    let dir = root;
    for (const part of parts) {
      const path = dir.path ? `${dir.path}/${part}` : part;
      let next = dir.children.find(
        (c): c is DirNode => c.kind === "dir" && c.name === part,
      );
      if (!next) {
        next = { kind: "dir", name: part, path, children: [], fileCount: 0 };
        dir.children.push(next);
      }
      dir = next;
    }
    dir.children.push({
      kind: "file",
      name: isDir ? `${leaf}/` : leaf,
      entry,
    });
  }
  return finish(root, sort).children;
}

function finish(dir: DirNode, sort: SortOrder): DirNode {
  let count = 0;
  for (const child of dir.children) {
    if (child.kind === "dir") count += finish(child, sort).fileCount;
    else count += 1;
  }
  dir.fileCount = count;
  dir.children.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
    if (a.kind === "file" && b.kind === "file" && sort === "status") {
      return byStatus(a.entry, b.entry) || compareNames(a.name, b.name);
    }
    return compareNames(a.name, b.name);
  });
  return dir;
}

/** Every folder path in the tree (for "collapse all"). */
export function dirPaths(nodes: readonly TreeNode[]): string[] {
  return nodes.flatMap((n) =>
    n.kind === "dir" ? [n.path, ...dirPaths(n.children)] : [],
  );
}

/** All file entries below the given nodes. */
export function filesUnder(nodes: readonly TreeNode[]): StatusEntry[] {
  return nodes.flatMap((n) =>
    n.kind === "dir" ? filesUnder(n.children) : [n.entry],
  );
}

export type VisibleRow = {
  node: TreeNode;
  level: number;
  /** Path of the parent folder ("" at the top). */
  parent: string;
};

/** The rows shown, in order, skipping the contents of collapsed folders. */
export function visibleRows(
  nodes: readonly TreeNode[],
  isCollapsed: (dirPath: string) => boolean,
  level = 1,
  parent = "",
): VisibleRow[] {
  return nodes.flatMap((node) => {
    const row: VisibleRow = { node, level, parent };
    if (node.kind === "file" || isCollapsed(node.path)) return [row];
    return [
      row,
      ...visibleRows(node.children, isCollapsed, level + 1, node.path),
    ];
  });
}
