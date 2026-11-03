import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { emptyRefs, oid, sampleRefs } from "../../test/ipc";
import { useGraphStore, type RepoGraph } from "../graph/store";
import { useStashStore } from "../stash/store";
import { Sidebar } from "./Sidebar";

function withRefs(refs: RepoGraph["refs"]) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs,
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

const section = (name: RegExp) =>
  screen.getByRole("button", { name }).closest("section") as HTMLElement;

describe("Sidebar", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("shows placeholders without a repository", () => {
    render(<Sidebar repoId={null} />);
    expect(screen.getAllByText("—")).toHaveLength(4);
  });

  it("lists local branches, remotes and tags", () => {
    withRefs(sampleRefs());
    render(<Sidebar repoId={1} />);

    const local = section(/Ramas locales/);
    const main = within(local).getByRole("button", { name: /^main/ });
    expect(main).toHaveAttribute("aria-current", "true");
    expect(main).toHaveAttribute(
      "title",
      "Rama actual: main\nAl día con «origin/main»",
    );
    expect(within(main).getByText("origin/main")).toBeVisible(); // upstream
    expect(
      within(local).getByRole("button", { name: "feature" }),
    ).toBeVisible();
    expect(within(local).getByText("2")).toBeVisible();

    const remotes = section(/Remotos/);
    expect(
      within(remotes).getByRole("heading", { name: "origin" }),
    ).toBeVisible();
    expect(within(remotes).getByRole("button", { name: "main" })).toBeVisible();

    expect(
      within(section(/Tags/)).getByRole("button", { name: "v1.0" }),
    ).toBeVisible();
  });

  it("selects the branch's commit in the graph", async () => {
    const user = userEvent.setup();
    withRefs(sampleRefs());
    render(<Sidebar repoId={1} />);
    await user.click(screen.getByRole("button", { name: "feature" }));
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("d"));
  });

  it("collapses a section", async () => {
    const user = userEvent.setup();
    withRefs(sampleRefs());
    render(<Sidebar repoId={1} />);
    const toggle = screen.getByRole("button", { name: /Tags/ });
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: "v1.0" })).toBeNull();
  });

  it("explains empty lists and a detached HEAD", () => {
    withRefs({ ...emptyRefs(), head: { type: "detached", oid: oid("c") } });
    render(<Sidebar repoId={1} />);
    expect(screen.getByRole("button", { name: /HEAD separado/ })).toBeVisible();
    expect(screen.getByText("Sin remotos")).toBeVisible();
    expect(screen.getByText("Sin tags")).toBeVisible();
    // Not read yet: a dash; read and empty: said so.
    expect(screen.getAllByText("—")).toHaveLength(1);
    act(() =>
      useStashStore.setState({
        stashes: { 1: { entries: [], error: null, readAt: 0 } },
      }),
    );
    expect(screen.getByText("Sin entradas")).toBeVisible();
  });
});
