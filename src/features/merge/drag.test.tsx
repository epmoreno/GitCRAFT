import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { MovePreview } from "../../ipc";
import { cleanStatus, ipc, oid, sampleRefs } from "../../test/ipc";
import { RefChips } from "../graph/RefChips";
import { labelsByCommit } from "../graph/refLabels";
import { useGraphStore } from "../graph/store";
import { Sidebar } from "../sidebar/Sidebar";
import { useWorkdirStore } from "../workdir/store";
import { DragLayer } from "./DragLayer";
import { useDragStore } from "./drag";
import { MergeDialog } from "./MergeDialog";

const tip = (refname: string, remote = false) => ({
  refname,
  name: refname.replace(/^refs\/(heads|remotes)\//, ""),
  remote,
  oid: oid("d"),
  short: "ddddddd",
});

function preview(
  kind: "merge" | "rebase",
  source: string,
  target: string,
): MovePreview {
  return {
    kind,
    source: tip(source, source.startsWith("refs/remotes/")),
    target: tip(target),
    current: "main",
    moving: kind === "merge" ? tip(target).name : tip(source).name,
    refused: null,
    case: kind,
    commits: [],
    commitsTotal: 0,
    skipped: 0,
    publishedIn: [],
    blocked: null,
    blocking: [],
    conflicts: [],
    stopsAt: null,
    exact: true,
    message: kind === "merge" ? "Merge branch 'x'" : null,
    fingerprint: "fp",
  };
}

function setup() {
  render(
    <>
      <Sidebar repoId={1} />
      <MergeDialog />
      <DragLayer />
    </>,
  );
  const local = screen
    .getByRole("button", { name: /Ramas locales/ })
    .closest("section") as HTMLElement;
  const remotes = screen
    .getByRole("button", { name: /Remotos/ })
    .closest("section") as HTMLElement;
  return {
    feature: within(local).getByRole("button", { name: /^feature/ }),
    main: within(local).getByRole("button", { name: /^main/ }),
    originMain: within(remotes).getByRole("button", { name: "main" }),
  };
}

/** Press on `from`, move onto `to` (past the threshold). */
function dragOnto(from: HTMLElement, to: HTMLElement) {
  fireEvent.pointerDown(from, { button: 0, clientX: 10, clientY: 10 });
  fireEvent.pointerMove(to, { clientX: 10, clientY: 60 });
}

afterEach(() => {
  // A test that failed mid-drag must not leave its listeners behind.
  fireEvent(window, new Event("pointercancel"));
});

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: sampleRefs(),
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
  ipc.mergePreview.mockImplementation((_id, source, target) =>
    Promise.resolve({ status: "ok", data: preview("merge", source, target) }),
  );
  ipc.rebasePreview.mockImplementation((_id, source, target) =>
    Promise.resolve({ status: "ok", data: preview("rebase", source, target) }),
  );
});

describe("dragging a branch onto another", () => {
  it("a click is not a drag: it selects, as before", () => {
    const { feature } = setup();
    fireEvent.pointerDown(feature, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(feature, { clientX: 12, clientY: 11 });
    fireEvent.pointerUp(feature, { clientX: 12, clientY: 11 });
    fireEvent.click(feature);
    expect(useDragStore.getState().dragging).toBeNull();
    expect(ipc.mergePreview).not.toHaveBeenCalled();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("d"));
  });

  it("shows the branch under the pointer and the valid targets", () => {
    const { feature, main, originMain } = setup();
    dragOnto(feature, main);
    expect(
      screen.getByRole("status", { name: "Arrastrando la rama feature" }),
    ).toHaveTextContent("⑂ feature");
    expect(screen.getByRole("status")).toHaveTextContent(
      "Suelta para hacer merge o rebase con «main»",
    );
    expect(main.className).toMatch(/dropOver/);
    // The dragged branch itself and remote branches are not targets.
    expect(feature.className).not.toMatch(/drop/);
    expect(originMain.className).not.toMatch(/drop/);
    expect(document.body.dataset.dragging).toBe("branch");
  });

  it("dropping opens the dialog with Merge chosen, Rebase one click away", async () => {
    const user = userEvent.setup();
    const { feature, main } = setup();
    dragOnto(feature, main);
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).toHaveBeenCalledWith(
      1,
      "refs/heads/feature",
      "refs/heads/main",
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: "¿Merge de «feature» en «main»?",
    });
    expect(useDragStore.getState().dragging).toBeNull();
    expect(document.body.dataset.dragging).toBeUndefined();
    const merge = within(dialog).getByRole("radio", { name: /^Merge:/ });
    expect(merge).toBeChecked();
    expect(dialog).toHaveTextContent(
      "«main» recibe los cambios de «feature» con un commit de merge",
    );
    await user.click(within(dialog).getByRole("radio", { name: /^Rebase:/ }));
    expect(ipc.rebasePreview).toHaveBeenCalledWith(
      1,
      "refs/heads/feature",
      "refs/heads/main",
    );
    expect(
      await screen.findByRole("alertdialog", {
        name: "¿Rebase de «feature» sobre «main»?",
      }),
    ).toBeInTheDocument();
  });

  it("the click after a drop does nothing", () => {
    const { feature, main } = setup();
    useGraphStore.getState().select(1, oid("a"));
    dragOnto(feature, main);
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    fireEvent.click(main);
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("a"));
  });

  it("Esc cancels, and dropping elsewhere does nothing", () => {
    const { feature, main, originMain } = setup();
    dragOnto(feature, main);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("status")).toBeNull();
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).not.toHaveBeenCalled();

    dragOnto(feature, originMain);
    expect(screen.getByRole("status")).not.toHaveTextContent("Suelta");
    expect(document.body.dataset.dragging).toBe("blocked");
    fireEvent.pointerUp(originMain, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).not.toHaveBeenCalled();

    // Tags do not move, and are not dragged.
    const tag = screen.getByRole("button", { name: "v1.0" });
    dragOnto(feature, tag);
    expect(document.body.dataset.dragging).toBe("blocked");
    fireEvent.pointerUp(tag, { clientX: 10, clientY: 60 });
    fireEvent.pointerDown(tag, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(main, { clientX: 10, clientY: 60 });
    expect(useDragStore.getState().dragging).toBeNull();
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("a remote branch can be dragged onto a local one", () => {
    const { main, originMain } = setup();
    dragOnto(originMain, main);
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).toHaveBeenCalledWith(
      1,
      "refs/remotes/origin/main",
      "refs/heads/main",
    );
  });

  it("works with the graph's branch labels, also onto the sidebar", () => {
    const labels = labelsByCommit(sampleRefs());
    const { main } = setup();
    render(<RefChips labels={labels.get(oid("d"))} color="red" repoId={1} />);
    render(<RefChips labels={labels.get(oid("e"))} color="red" repoId={1} />);
    const featureChip = screen
      .getAllByText("feature")
      .find((el) => el.tagName === "SPAN" && el.dataset.dropBranch)!;
    const mainChip = screen
      .getAllByText("main")
      .find((el) => el.tagName === "SPAN" && el.dataset.dropBranch)!;
    // Graph label onto graph label.
    dragOnto(featureChip, mainChip);
    expect(mainChip.className).toMatch(/dropOver/);
    fireEvent.pointerUp(mainChip, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).toHaveBeenLastCalledWith(
      1,
      "refs/heads/feature",
      "refs/heads/main",
    );
    // Graph label onto the sidebar.
    ipc.mergePreview.mockClear();
    dragOnto(featureChip, main);
    fireEvent.pointerUp(main, { clientX: 10, clientY: 60 });
    expect(ipc.mergePreview).toHaveBeenCalledWith(
      1,
      "refs/heads/feature",
      "refs/heads/main",
    );
  });
});
