import { readFileSync } from "node:fs";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { WorkingStatus } from "../../ipc";
import {
  cleanStatus,
  commandError,
  emptyRefs,
  ipc,
  sampleLines,
  streamEvents,
} from "../../test/ipc";
import { DetailPanel } from "../detail/DetailPanel";
import { GraphView } from "../graph/GraphView";
import { useGraphStore, WORKTREE } from "../graph/store";
import { changeCounts, useWorkdirStore } from "./store";
import { useWorkdirViewPrefs } from "./viewPrefs";
import { DiscardDialog } from "./Discard";
import { WorkingDirPanel } from "./WorkingDirPanel";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

export function busyStatus(): WorkingStatus {
  return {
    ...cleanStatus(),
    staged: [
      { path: "src/app.ts", oldPath: null, status: "modified" },
      { path: "docs/new name.md", oldPath: "docs/old.md", status: "renamed" },
    ],
    unstaged: [{ path: "README.md", oldPath: null, status: "modified" }],
    untracked: [{ path: "notes/", oldPath: null, status: "untracked" }],
    conflicted: [{ path: "conflict.txt", kind: "bothModified" }],
  };
}

describe("working directory store", () => {
  it("counts changes, untracked files as unstaged", () => {
    expect(changeCounts(busyStatus())).toEqual({
      staged: 2,
      unstaged: 2,
      conflicted: 1,
      total: 5,
    });
    expect(changeCounts(null).total).toBe(0);
  });

  it("keeps only the latest answer and the last good status on errors", async () => {
    let first:
      ((v: Awaited<ReturnType<typeof ipc.workingStatus>>) => void) | undefined;
    ipc.workingStatus
      .mockImplementationOnce(() => new Promise((resolve) => (first = resolve)))
      .mockResolvedValueOnce({ status: "ok", data: busyStatus() })
      .mockResolvedValueOnce({
        status: "error",
        error: commandError("unknown"),
      });

    const stale = useWorkdirStore.getState().refresh(1);
    await useWorkdirStore.getState().refresh(1);
    first?.({ status: "ok", data: cleanStatus() });
    await stale;
    expect(useWorkdirStore.getState().statuses[1]?.status).toEqual(
      busyStatus(),
    );

    await useWorkdirStore.getState().refresh(1);
    expect(useWorkdirStore.getState().statuses[1]?.status).toEqual(
      busyStatus(),
    );
    expect(useWorkdirStore.getState().statuses[1]?.error?.kind).toBe("unknown");
  });
});

describe("uncommitted changes", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
  });

  it("are not shown when the working directory is clean", async () => {
    useWorkdirStore.setState({
      statuses: { 1: { status: cleanStatus(), error: null } },
    });
    render(<GraphView repoId={1} info={info} />);
    await screen.findByText("6 commits");
    expect(
      screen.queryByRole("button", { name: /Cambios sin confirmar/ }),
    ).toBeNull();
  });

  it("appear at the top of the history and open the working directory panel", async () => {
    const user = userEvent.setup();
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
    render(
      <>
        <GraphView repoId={1} info={info} />
        <DetailPanel repoId={1} />
      </>,
    );
    const row = await screen.findByRole("button", {
      name: /Cambios sin confirmar/,
    });
    expect(row).toHaveTextContent("1 conflicto");
    expect(row).toHaveTextContent("2 preparados");
    expect(row).toHaveTextContent("2 sin preparar");

    await user.click(row);
    expect(row).toHaveAttribute("aria-pressed", "true");
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(WORKTREE);
    expect(ipc.commitDetails).not.toHaveBeenCalled();

    const panel = screen.getByRole("complementary", { name: "Detalle" });
    const conflicts = within(panel).getByRole("region", { name: "Conflictos" });
    expect(conflicts).toHaveTextContent("conflict.txtmodificado en ambos");
    const staged = within(panel).getByRole("region", { name: "Preparados" });
    expect(within(staged).getAllByRole("treeitem")).toHaveLength(2);
    expect(staged).toHaveTextContent("antes: docs/old.md");
    const unstaged = within(panel).getByRole("region", {
      name: "Sin preparar",
    });
    expect(
      within(unstaged).getByLabelText("Sin seguimiento"),
    ).toHaveTextContent("?");
    expect(within(unstaged).getByLabelText("Modificado")).toBeVisible();
  });
});

describe("staging files", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    // Re-reads after each action keep returning the same busy status.
    ipc.workingStatus.mockResolvedValue({ status: "ok", data: busyStatus() });
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
  });

  const panel = () => {
    render(<WorkingDirPanel repoId={1} />);
    return {
      unstaged: screen.getByRole("region", { name: "Sin preparar" }),
      staged: screen.getByRole("region", { name: "Preparados" }),
    };
  };

  it("stages and unstages single files, then re-reads the status", async () => {
    const user = userEvent.setup();
    const { unstaged, staged } = panel();
    const reads = ipc.workingStatus.mock.calls.length;

    await user.click(
      within(unstaged).getByRole("button", { name: "Preparar README.md" }),
    );
    expect(ipc.stageFiles).toHaveBeenCalledWith(1, ["README.md"]);
    expect(ipc.workingStatus.mock.calls.length).toBe(reads + 1);

    await user.click(
      within(unstaged).getByRole("button", { name: "Preparar notes/" }),
    );
    expect(ipc.stageFiles).toHaveBeenLastCalledWith(1, ["notes/"]);

    await user.click(
      within(staged).getByRole("button", {
        name: "Quitar src/app.ts del stage",
      }),
    );
    expect(ipc.unstageFiles).toHaveBeenCalledWith(1, ["src/app.ts"]);
  });

  it("unstages a rename under both names", async () => {
    const user = userEvent.setup();
    const { staged } = panel();
    await user.click(
      within(staged).getByRole("button", {
        name: "Quitar docs/new name.md del stage",
      }),
    );
    expect(ipc.unstageFiles).toHaveBeenCalledWith(1, [
      "docs/new name.md",
      "docs/old.md",
    ]);
  });

  it("stages or unstages everything at once", async () => {
    const user = userEvent.setup();
    const { unstaged, staged } = panel();
    await user.click(
      within(unstaged).getByRole("button", { name: "Preparar todo" }),
    );
    expect(ipc.stageFiles).toHaveBeenCalledWith(1, null);
    await user.click(
      within(staged).getByRole("button", { name: "Quitar todo" }),
    );
    expect(ipc.unstageFiles).toHaveBeenCalledWith(1, null);
  });

  it("stages a file on double click", async () => {
    const user = userEvent.setup();
    const { unstaged } = panel();
    await user.dblClick(within(unstaged).getByText("README.md"));
    expect(ipc.stageFiles).toHaveBeenCalledWith(1, ["README.md"]);
  });

  it("does not offer staging for conflicts", () => {
    panel();
    const conflicts = screen.getByRole("region", { name: "Conflictos" });
    // Only the button that folds the section.
    expect(within(conflicts).getAllByRole("button")).toEqual([
      within(conflicts).getByRole("button", { name: /Conflictos/ }),
    ]);
  });

  it("shows failures with git's output until dismissed", async () => {
    const user = userEvent.setup();
    ipc.stageFiles.mockResolvedValue({
      status: "error",
      error: commandError(
        "unknown",
        "$ git add\nfatal: Unable to create index.lock",
      ),
    });
    const { unstaged } = panel();
    await user.click(
      within(unstaged).getByRole("button", { name: "Preparar README.md" }),
    );
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/index.lock/)).toHaveClass("selectable");
    await user.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("panel layout", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.workingStatus.mockResolvedValue({ status: "ok", data: busyStatus() });
  });

  const show = (status: WorkingStatus = busyStatus()) => {
    useWorkdirStore.setState({ statuses: { 1: { status, error: null } } });
    return render(<WorkingDirPanel repoId={1} />);
  };

  it("says how many changes there are and on which branch", () => {
    show();
    expect(screen.getByText("5 cambios en main")).toBeVisible();
  });

  it("says when HEAD is detached", () => {
    const status = busyStatus();
    status.branch = { ...status.branch, name: null };
    show(status);
    expect(screen.getByText("5 cambios en HEAD separado")).toBeVisible();
  });

  it("keeps the commit box outside the scrolling list, below a divider", () => {
    const { container } = show();
    const unstaged = screen.getByRole("region", { name: "Sin preparar" });
    const divider = screen.getByRole("separator", {
      name: "Altura de la caja de commit",
    });
    const summary = screen.getByRole("textbox", { name: "Resumen" });
    const list = unstaged.parentElement!;
    expect(list).not.toContainElement(summary);
    expect(divider.previousElementSibling).toBe(list);
    expect(divider.nextElementSibling).toContainElement(summary);
    expect(container).toContainElement(divider);
  });

  it("resizes the commit box with the keyboard and remembers it", async () => {
    const user = userEvent.setup();
    show();
    const divider = screen.getByRole("separator");
    const area = () => divider.nextElementSibling as HTMLElement;
    // jsdom has no layout: the natural height reads as the minimum.
    expect(divider).toHaveAttribute("aria-valuenow", "140");
    // The panel is 600px high in tests (setup.ts): at most 600 - 120.
    expect(divider).toHaveAttribute("aria-valuemax", "480");

    divider.focus();
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(area()).toHaveStyle({ height: "172px" });
    expect(divider).toHaveAttribute("aria-valuenow", "172");
    await user.keyboard("{Home}");
    expect(area()).toHaveStyle({ height: "480px" });
    await user.keyboard("{End}");
    expect(area()).toHaveStyle({ height: "140px" });
    expect(
      JSON.parse(localStorage.getItem("gitcraft.workdir-view") ?? "{}").state,
    ).toMatchObject({ commitBoxHeight: 140 });

    await user.keyboard("{Enter}");
    expect(area().style.height).toBe("");
    expect(useWorkdirViewPrefs.getState().commitBoxHeight).toBeNull();
  });
});

describe("collapsible sections", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.workingStatus.mockResolvedValue({ status: "ok", data: busyStatus() });
  });

  it("folds a section, keeps its count and actions, and remembers it", async () => {
    const user = userEvent.setup();
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
    const { unmount } = render(<WorkingDirPanel repoId={1} />);
    const unstaged = screen.getByRole("region", { name: "Sin preparar" });
    const toggle = within(unstaged).getByRole("button", {
      name: "Sin preparar 2",
    });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(within(unstaged).getByText("README.md")).not.toBeVisible();
    expect(
      within(unstaged).getByRole("button", { name: "Preparar todo" }),
    ).toBeEnabled();
    expect(
      within(unstaged).getByRole("button", { name: "Descartar todo" }),
    ).toBeEnabled();

    // The staged section is not affected, and the choice survives a remount.
    expect(
      within(screen.getByRole("region", { name: "Preparados" })).getByText(
        "app.ts",
      ),
    ).toBeVisible();
    unmount();
    render(<WorkingDirPanel repoId={1} />);
    expect(
      screen.getByRole("button", { name: "Sin preparar 2" }),
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      JSON.parse(localStorage.getItem("gitcraft.workdir-view") ?? "{}").state
        .collapsedSections,
    ).toEqual({ unstaged: true });
  });

  it("shows the actions of an empty section, disabled", () => {
    useWorkdirStore.setState({
      statuses: { 1: { status: cleanStatus(), error: null } },
    });
    render(<WorkingDirPanel repoId={1} />);
    const staged = screen.getByRole("region", { name: "Preparados" });
    expect(
      within(staged).getByRole("button", { name: "Quitar todo" }),
    ).toBeDisabled();
    const unstaged = screen.getByRole("region", { name: "Sin preparar" });
    expect(
      within(unstaged).getByRole("button", { name: "Preparar todo" }),
    ).toBeDisabled();
    expect(
      within(unstaged).getByRole("button", { name: "Descartar todo" }),
    ).toBeDisabled();
  });

  it("discards everything unstaged through the confirmation", async () => {
    const user = userEvent.setup();
    ipc.discardPreview.mockReturnValue(new Promise(() => {}));
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
    render(<WorkingDirPanel repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Descartar todo" }));
    expect(ipc.discardPreview).toHaveBeenCalledWith(1, {
      kind: "files",
      paths: ["README.md", "notes/"],
    });
    expect(ipc.discardChanges).not.toHaveBeenCalled();
  });
});

describe("tree view", () => {
  const treeStatus = (): WorkingStatus => ({
    ...cleanStatus(),
    unstaged: [
      { path: "src/ui/Button.tsx", oldPath: null, status: "modified" },
      { path: "src/ui/Menu.tsx", oldPath: null, status: "modified" },
      { path: "src/main.ts", oldPath: null, status: "modified" },
      { path: "README.md", oldPath: null, status: "modified" },
    ],
    staged: [{ path: "docs/guide.md", oldPath: null, status: "added" }],
  });

  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.workingStatus.mockResolvedValue({ status: "ok", data: treeStatus() });
    useWorkdirStore.setState({
      statuses: { 1: { status: treeStatus(), error: null } },
    });
  });

  const showTree = async () => {
    const user = userEvent.setup();
    const view = render(<WorkingDirPanel repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Árbol" }));
    const tree = screen.getByRole("tree", { name: "Sin preparar" });
    const item = (name: string | RegExp) =>
      within(tree).getByRole("treeitem", { name });
    return { user, tree, item, ...view };
  };

  it("switches between list and tree, and remembers the choice", async () => {
    const { user, unmount } = await showTree();
    expect(screen.getByRole("button", { name: "Árbol" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    unmount();
    render(<WorkingDirPanel repoId={1} />);
    expect(screen.getByRole("tree", { name: "Sin preparar" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Lista" }));
    // The list is a flat tree: no folders, full paths.
    const list = screen.getByRole("tree", { name: "Sin preparar" });
    for (const row of within(list).getAllByRole("treeitem")) {
      expect(row).toHaveAttribute("aria-level", "1");
      expect(row).not.toHaveAttribute("aria-expanded");
    }
    expect(screen.getAllByText("src/ui/")).toHaveLength(2);
  });

  it("shows folders with their file count, names without the folder", async () => {
    const { tree, item } = await showTree();
    const rows = within(tree).getAllByRole("treeitem");
    expect(rows.map((r) => r.getAttribute("aria-level"))).toEqual([
      "1",
      "2",
      "3",
      "3",
      "2",
      "1",
    ]);
    expect(item(/^src/)).toHaveTextContent("src3");
    expect(item(/^ui/)).toHaveTextContent("ui2");
    expect(within(item(/^ui/)).getByTitle("2 archivos")).toBeInTheDocument();
    expect(item(/Button\.tsx/)).not.toHaveTextContent("src/ui/");
  });

  it("collapses folders by click, and all at once", async () => {
    const { user, tree, item } = await showTree();
    await user.click(item(/^ui/));
    expect(item(/^ui/)).toHaveAttribute("aria-expanded", "false");
    expect(within(tree).queryByText("Button.tsx")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Expandir todo" }));
    expect(within(tree).getByText("Button.tsx")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Plegar todo" }));
    expect(within(tree).getAllByRole("treeitem")).toHaveLength(2);
    const staged = screen.getByRole("tree", { name: "Preparados" });
    expect(within(staged).getAllByRole("treeitem")).toHaveLength(1);
  });

  it("is navigable with the keyboard", async () => {
    useGraphStore.setState({
      graphs: {
        1: {
          status: "done",
          lines: [],
          maxColumn: 0,
          refs: emptyRefs(),
          error: null,
          selectedOid: WORKTREE,
          suggestOptimize: false,
          openFile: null,
          loadId: 1,
        },
      },
    });
    const { user, item } = await showTree();
    const src = item(/^src/);
    expect(src).toHaveAttribute("tabindex", "0");
    expect(item(/^ui/)).toHaveAttribute("tabindex", "-1");
    src.focus();

    await user.keyboard("{ArrowLeft}");
    expect(src).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{ArrowRight}");
    expect(src).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowRight}");
    expect(item(/^ui/)).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(item(/Button\.tsx/)).toHaveFocus();
    expect(item(/Button\.tsx/)).toHaveAttribute("tabindex", "0");
    await user.keyboard("{ArrowLeft}");
    expect(item(/^ui/)).toHaveFocus();
    await user.keyboard("{End}");
    expect(item(/README/)).toHaveFocus();

    await user.keyboard("{Home}{ArrowDown}{ArrowDown}");
    await user.keyboard("{Enter}");
    expect(useGraphStore.getState().graphs[1]?.openFile).toMatchObject({
      kind: "worktree",
      area: "unstaged",
      path: "src/ui/Button.tsx",
    });
    await user.keyboard(" ");
    expect(ipc.stageFiles).toHaveBeenCalledWith(1, ["src/ui/Button.tsx"]);
  });

  it("discards the focused file with ⌘⌫ (not ⌫ or Delete alone), after the preview", async () => {
    ipc.discardPreview.mockResolvedValue({
      status: "ok",
      data: [
        {
          path: "README.md",
          untracked: false,
          added: 1,
          removed: 0,
          lines: null,
          fileCount: null,
          undoUnsupported: false,
        },
      ],
    });
    const { user, item } = await showTree();
    render(<DiscardDialog />);
    // Clicking the name focuses the whole row, where the shortcut lives.
    await user.click(within(item(/README/)).getByText("README.md"));
    expect(item(/README/)).toHaveFocus();
    await user.keyboard("{Backspace}{Delete}");
    expect(ipc.discardPreview).not.toHaveBeenCalled();
    await user.keyboard("{Meta>}{Backspace}{/Meta}");
    expect(ipc.discardPreview).toHaveBeenCalledWith(1, {
      kind: "files",
      paths: ["README.md"],
    });
    // The usual confirmation, and nothing discarded until it is accepted.
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("README.md");
    expect(ipc.discardChanges).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));

    // Staged files and folders have nothing to discard from here.
    const staged = screen.getByRole("tree", { name: "Preparados" });
    within(staged).getByRole("treeitem", { name: /guide/ }).focus();
    await user.keyboard("{Meta>}{Backspace}{/Meta}");
    item(/^src/).focus();
    await user.keyboard("{Meta>}{Backspace}{/Meta}");
    expect(ipc.discardPreview).toHaveBeenCalledTimes(1);

    // The same in the list view.
    await user.click(screen.getByRole("button", { name: "Lista" }));
    const list = screen.getByRole("tree", { name: "Sin preparar" });
    const row = within(list).getByRole("treeitem", { name: /main\.ts/ });
    await user.click(within(row).getByText("main.ts"));
    expect(row).toHaveFocus();
    await user.keyboard("{Meta>}{Backspace}{/Meta}");
    expect(ipc.discardPreview).toHaveBeenLastCalledWith(1, {
      kind: "files",
      paths: ["src/main.ts"],
    });
  });
});

describe("sorting and whole folders", () => {
  const status = (): WorkingStatus => ({
    ...cleanStatus(),
    unstaged: [
      { path: "-dir/z b.txt", oldPath: null, status: "modified" },
      { path: "-dir/sub/a.txt", oldPath: null, status: "deleted" },
      { path: "lib.ts", oldPath: null, status: "modified" },
    ],
    untracked: [
      { path: "-dir/new file.txt", oldPath: null, status: "untracked" },
    ],
    staged: [
      { path: "docs/new name.md", oldPath: "docs/old.md", status: "renamed" },
      { path: "docs/a.md", oldPath: null, status: "added" },
    ],
  });

  beforeEach(async () => {
    await i18n.changeLanguage("es");
    ipc.workingStatus.mockResolvedValue({ status: "ok", data: status() });
    useWorkdirStore.setState({
      statuses: { 1: { status: status(), error: null } },
    });
  });

  const paths = (region: string) =>
    within(screen.getByRole("region", { name: region }))
      .getAllByRole("treeitem")
      .map((li) => li.querySelector(".path")?.getAttribute("title"));

  it("sorts the list by name or by status, and remembers it", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<WorkingDirPanel repoId={1} />);
    expect(paths("Sin preparar")).toEqual([
      "-dir/new file.txt",
      "-dir/sub/a.txt",
      "-dir/z b.txt",
      "lib.ts",
    ]);
    await user.click(screen.getByRole("button", { name: "Estado" }));
    expect(paths("Sin preparar")).toEqual([
      "-dir/z b.txt",
      "lib.ts",
      "-dir/sub/a.txt",
      "-dir/new file.txt",
    ]);
    unmount();
    render(<WorkingDirPanel repoId={1} />);
    expect(screen.getByRole("button", { name: "Estado" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("stages a whole folder by listing its files", async () => {
    const user = userEvent.setup();
    render(<WorkingDirPanel repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Árbol" }));
    const tree = screen.getByRole("tree", { name: "Sin preparar" });
    const folder = within(tree).getByRole("treeitem", { name: /^-dir/ });
    await user.click(
      within(folder).getByRole("button", { name: "Preparar la carpeta -dir" }),
    );
    expect(ipc.stageFiles).toHaveBeenCalledWith(1, [
      "-dir/sub/a.txt",
      "-dir/new file.txt",
      "-dir/z b.txt",
    ]);
    // The button does not fold the folder.
    expect(folder).toHaveAttribute("aria-expanded", "true");

    // Also with Space on the folder, and from its context menu.
    const sub = within(tree).getByRole("treeitem", { name: /^sub/ });
    sub.focus();
    await user.keyboard(" ");
    expect(ipc.stageFiles).toHaveBeenLastCalledWith(1, ["-dir/sub/a.txt"]);
    await user.pointer({ keys: "[MouseRight]", target: sub });
    await user.click(
      screen.getByRole("menuitem", { name: "Preparar la carpeta -dir/sub" }),
    );
    expect(ipc.stageFiles).toHaveBeenCalledTimes(3);
    expect(sub).toHaveAttribute("aria-expanded", "true");
  });

  it("unstages a folder with renames under both names", async () => {
    const user = userEvent.setup();
    render(<WorkingDirPanel repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Árbol" }));
    const tree = screen.getByRole("tree", { name: "Preparados" });
    await user.click(
      within(tree).getByRole("button", {
        name: "Quitar la carpeta docs del stage",
      }),
    );
    expect(ipc.unstageFiles).toHaveBeenCalledWith(1, [
      "docs/a.md",
      "docs/new name.md",
      "docs/old.md",
    ]);
  });
});

describe("panel header controls", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
  });

  it("uses icon buttons with tooltips for expand and collapse all", async () => {
    const user = userEvent.setup();
    render(<WorkingDirPanel repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Árbol" }));
    for (const name of ["Expandir todo", "Plegar todo"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toHaveAccessibleDescription(name);
      expect(button).toHaveTextContent("");
      expect(button.querySelector("svg")).not.toBeNull();
    }
  });

  it("wraps instead of cutting labels (no layout in jsdom: checks the CSS)", () => {
    const css = readFileSync(
      `${import.meta.dirname}/workdir.module.css`,
      "utf8",
    );
    const rule = (selector: string) =>
      css.match(new RegExp(`\\n\\${selector} \\{([^}]*)\\}`))?.[1] ?? "";
    expect(rule(".viewControls")).toMatch(/flex-wrap: wrap/);
    expect(rule(".segmented")).toMatch(/flex: none/);
    expect(rule(".segment")).toMatch(/white-space: nowrap/);
  });
});
