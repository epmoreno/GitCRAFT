import type { StatusEntry } from "../../ipc";
import {
  buildTree,
  dirPaths,
  filesUnder,
  visibleRows,
  type DirNode,
} from "./fileTree";

const e = (path: string, status: StatusEntry["status"] = "modified") => ({
  path,
  oldPath: null,
  status,
});

describe("file tree", () => {
  const entries = [
    e("src/ui/b.ts"),
    e("README.md"),
    e("src/app.ts"),
    e("src/ui/a 2.ts"),
    e("src/ui/a 10.ts"),
    e("notes/", "untracked"),
    e("-dash/x.txt", "added"),
  ];

  it("groups paths into folders, folders first, with file counts", () => {
    const tree = buildTree(entries);
    expect(tree.map((n) => n.name)).toEqual([
      "-dash",
      "src",
      "notes/",
      "README.md",
    ]);
    const src = tree[1] as DirNode;
    expect(src.fileCount).toBe(4);
    expect(src.children.map((n) => n.name)).toEqual(["ui", "app.ts"]);
    const ui = src.children[0] as DirNode;
    expect(ui.path).toBe("src/ui");
    expect(ui.fileCount).toBe(3);
    // Numbers in names sort naturally.
    expect(ui.children.map((n) => n.name)).toEqual([
      "a 2.ts",
      "a 10.ts",
      "b.ts",
    ]);
  });

  it("keeps an untracked directory as a single leaf", () => {
    const notes = buildTree(entries).find((n) => n.name === "notes/");
    expect(notes?.kind).toBe("file");
  });

  it("lists folders and the files below them", () => {
    const tree = buildTree(entries);
    expect(dirPaths(tree)).toEqual(["-dash", "src", "src/ui"]);
    expect(filesUnder([tree[1]!]).map((f) => f.path)).toEqual([
      "src/ui/a 2.ts",
      "src/ui/a 10.ts",
      "src/ui/b.ts",
      "src/app.ts",
    ]);
  });

  it("hides the contents of collapsed folders", () => {
    const rows = visibleRows(buildTree(entries), (p) => p === "src/ui");
    expect(rows.map((r) => [r.node.name, r.level, r.parent])).toEqual([
      ["-dash", 1, ""],
      ["x.txt", 2, "-dash"],
      ["src", 1, ""],
      ["ui", 2, "src"],
      ["app.ts", 2, "src"],
      ["notes/", 1, ""],
      ["README.md", 1, ""],
    ]);
  });
});
