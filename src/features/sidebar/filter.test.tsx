import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { StashEntry } from "../../ipc";
import { oid, sampleRefs } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { useStashStore } from "../stash/store";
import { Sidebar } from "./Sidebar";

const stash = (message: string, branch: string): StashEntry => ({
  index: 0,
  oid: oid(message[0] ?? "0"),
  message,
  branch,
  time: Math.floor(Date.now() / 1000),
  untracked: null,
  origin: "user",
});

function renderSidebar(repoId = 1) {
  const refs = sampleRefs();
  refs.refs.push({
    ...refs.refs[2]!,
    name: "refs/remotes/origin/login-fix",
    shortName: "origin/login-fix",
  });
  useGraphStore.setState({
    graphs: Object.fromEntries(
      [1, 2].map((id) => [
        id,
        {
          status: "done",
          lines: [],
          maxColumn: 0,
          refs,
          error: null,
          selectedOid: null,
          suggestOptimize: false,
          openFile: null,
          loadId: id,
        },
      ]),
    ),
  });
  useStashStore.setState({
    stashes: {
      1: {
        entries: [
          stash("arreglo del login", "main"),
          stash("otra cosa", "feature"),
        ],
        error: null,
        readAt: Date.now(),
      },
    },
  });
  return render(<Sidebar repoId={repoId} />);
}

const section = (name: RegExp) =>
  screen.getByRole("button", { name }).closest("section") as HTMLElement;

describe("sidebar filter", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("filters branches, remote branches, tags and stashes", async () => {
    const user = userEvent.setup();
    renderSidebar();
    const filter = screen.getByRole("searchbox", {
      name: "Filtrar la barra lateral",
    });

    await user.type(filter, "LOGIN");
    const local = section(/Ramas locales/);
    expect(
      within(local).getByText("Nada coincide con el filtro"),
    ).toBeVisible();
    expect(within(local).getByText("0/2")).toBeVisible();

    const remotes = section(/Remotos/);
    expect(
      within(remotes).getByRole("button", { name: "login-fix" }),
    ).toBeVisible();
    expect(
      within(remotes).queryByRole("button", { name: "main" }),
    ).not.toBeInTheDocument();

    expect(
      within(section(/Tags/)).getByText("Nada coincide con el filtro"),
    ).toBeVisible();
    const stashes = section(/Stashes/);
    expect(within(stashes).getByText("1/2")).toBeVisible();
    expect(within(stashes).getByText("arreglo del login")).toBeVisible();
    expect(within(stashes).queryByText("otra cosa")).not.toBeInTheDocument();

    // A remote's name shows all its branches; the upstream matches too.
    await user.clear(filter);
    await user.type(filter, "origin");
    expect(
      within(section(/Remotos/)).getByRole("button", { name: "main" }),
    ).toBeVisible();
    expect(
      within(section(/Ramas locales/)).getByRole("button", { name: /^main/ }),
    ).toBeVisible();
    expect(
      within(section(/Ramas locales/)).queryByRole("button", {
        name: /^feature/,
      }),
    ).not.toBeInTheDocument();
  });

  it("opens collapsed sections while filtering and clears with Esc", async () => {
    const user = userEvent.setup();
    renderSidebar();
    await user.click(screen.getByRole("button", { name: /Tags/ }));
    expect(within(section(/Tags/)).queryByText("v1.0")).not.toBeInTheDocument();

    const filter = screen.getByRole("searchbox");
    await user.type(filter, "v1");
    expect(within(section(/Tags/)).getByText("v1.0")).toBeVisible();

    const closed = vi.fn();
    window.addEventListener("keydown", closed);
    await user.keyboard("{Escape}");
    window.removeEventListener("keydown", closed);
    expect(filter).toHaveValue("");
    expect(closed).not.toHaveBeenCalled(); // e.g. an open diff stays open
    // Back to the collapsed state chosen before.
    expect(within(section(/Tags/)).queryByText("v1.0")).not.toBeInTheDocument();
  });

  it("keeps each tab's filter", async () => {
    const user = userEvent.setup();
    const { rerender } = renderSidebar(1);
    await user.type(screen.getByRole("searchbox"), "feat");
    act(() => rerender(<Sidebar repoId={2} />));
    expect(screen.getByRole("searchbox")).toHaveValue("");
    act(() => rerender(<Sidebar repoId={1} />));
    expect(screen.getByRole("searchbox")).toHaveValue("feat");
  });
});
