import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef } from "react";
import i18n from "../../i18n";
import { App } from "../../app/App";
import { commandError, ipc, opened } from "../../test/ipc";
import { RestoreFailedBanner } from "./OpenErrorBanner";
import { useRepoStore } from "./store";
import { TabBar } from "./TabBar";
import { forgetView, useRememberedScroll } from "./viewState";

const store = () => useRepoStore.getState();
const ids = () => store().tabs.map((t) => t.id);

function withTabs(count: number, activeId = 1) {
  useRepoStore.setState({
    tabs: Array.from({ length: count }, (_, i) =>
      opened(i + 1, `/repos/r${i + 1}`),
    ),
    activeId,
    restored: true,
  });
}

describe("tabs", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("restores the previous session with its active tab", async () => {
    ipc.restoreSession.mockResolvedValue({
      status: "ok",
      data: {
        opened: [opened(4, "/b"), opened(5, "/a")],
        active: 5,
        failed: [{ path: "/gone", error: commandError("pathNotFound") }],
      },
    });
    await store().restoreSession();
    expect(ids()).toEqual([4, 5]);
    expect(store().activeId).toBe(5);
    expect(store().restoreFailures).toHaveLength(1);

    render(<RestoreFailedBanner />);
    expect(
      screen.getByText(
        "No se pudo reabrir 1 repositorio de la sesión anterior",
      ),
    ).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "La carpeta «/gone» no existe",
    );
    await userEvent.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("saves the open tabs and the active one, only after restoring", async () => {
    useRepoStore.setState({ tabs: [opened(1, "/a")], activeId: 1 });
    expect(ipc.saveSession).not.toHaveBeenCalled();
    await store().restoreSession();
    expect(ipc.saveSession).toHaveBeenLastCalledWith(["/a"], "/a");

    useRepoStore.setState({
      tabs: [opened(1, "/a"), opened(2, "/b")],
      activeId: 2,
    });
    expect(ipc.saveSession).toHaveBeenLastCalledWith(["/a", "/b"], "/b");
    store().moveTab(2, 0);
    expect(ipc.saveSession).toHaveBeenLastCalledWith(["/b", "/a"], "/b");
  });

  it("moves, cycles and picks tabs", async () => {
    withTabs(4);
    store().moveTab(1, 2);
    expect(ids()).toEqual([2, 3, 1, 4]);
    store().moveTab(4, 0);
    expect(ids()).toEqual([4, 2, 3, 1]);
    store().moveTab(4, 99);
    expect(ids()).toEqual([2, 3, 1, 4]);

    store().activateIndex(0);
    expect(store().activeId).toBe(2);
    store().activateNext(-1);
    expect(store().activeId).toBe(4);
    store().activateNext(1);
    expect(store().activeId).toBe(2);
    store().activateIndex(8);
    expect(store().activeId).toBe(2);

    await store().closeActiveTab();
    expect(ids()).toEqual([3, 1, 4]);
    expect(ipc.closeRepository).toHaveBeenCalledWith(2);
  });

  it("reorders by dragging a tab", () => {
    withTabs(3);
    render(<TabBar />);
    const tabs = screen.getAllByRole("tab");
    // Three 100-px tabs side by side.
    tabs.forEach((tab, i) => {
      tab.parentElement!.getBoundingClientRect = () =>
        ({ left: i * 100, width: 100 }) as DOMRect;
    });
    const first = tabs[0]!.parentElement!;
    act(() => {
      first.dispatchEvent(
        new MouseEvent("pointerdown", {
          bubbles: true,
          clientX: 50,
          button: 0,
        }),
      );
      first.dispatchEvent(
        new MouseEvent("pointermove", { bubbles: true, clientX: 260 }),
      );
      first.dispatchEvent(
        new MouseEvent("pointerup", { bubbles: true, clientX: 260 }),
      );
    });
    expect(ids()).toEqual([2, 3, 1]);
    expect(store().activeId).toBe(1);
  });

  it("reorders and closes from the tab's menu", async () => {
    const user = userEvent.setup();
    withTabs(3, 2);
    render(<TabBar />);
    await user.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("tab", { name: "r1" }),
    });
    expect(
      screen.getByRole("menuitem", { name: "Mover a la izquierda" }),
    ).toBeDisabled();
    await user.click(
      screen.getByRole("menuitem", { name: "Mover a la derecha" }),
    );
    expect(ids()).toEqual([2, 1, 3]);

    await user.click(screen.getByRole("tab", { name: "r3" }));
    expect(store().activeId).toBe(3);
    expect(screen.getByRole("tab", { name: "r3" })).toHaveAttribute(
      "title",
      "/repos/r3\n⌘3",
    );
  });

  it("switches and closes tabs with the keyboard, but not over a dialog", async () => {
    withTabs(3);
    render(<App />);
    const key = (init: KeyboardEventInit) =>
      act(() => {
        window.dispatchEvent(new KeyboardEvent("keydown", init));
      });
    key({ key: "3", metaKey: true });
    expect(store().activeId).toBe(3);
    key({ key: "Tab", ctrlKey: true });
    expect(store().activeId).toBe(1);
    key({ key: "Tab", ctrlKey: true, shiftKey: true });
    expect(store().activeId).toBe(3);

    const modal = document.createElement("div");
    modal.setAttribute("aria-modal", "true");
    document.body.append(modal);
    key({ key: "1", metaKey: true });
    expect(store().activeId).toBe(3);
    modal.remove();

    const { installAppMenu } = await import("../../app/menu");
    const [, actions] = vi.mocked(installAppMenu).mock.lastCall!;
    await act(async () => actions.closeTab());
    expect(ids()).toEqual([1, 2]);
  });
});

describe("new tab", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  const tab = (name: string | RegExp) => screen.getByRole("tab", { name });

  it("opens a tab with the start page from + and ⌘T, only one at a time", async () => {
    const user = userEvent.setup();
    withTabs(2, 2);
    render(<App />);
    await user.click(screen.getByRole("button", { name: "Nueva pestaña" }));
    expect(tab("Nueva pestaña")).toHaveAttribute("aria-selected", "true");
    expect(store().activeId).toBeNull();
    expect(
      screen.getByRole("button", { name: "Abrir repositorio…" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Clonar repositorio…" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Inicializar repositorio…" }),
    ).toBeVisible();
    expect(screen.getByRole("heading", { name: "Recientes" })).toBeVisible();

    act(() => store().activate(1));
    const { installAppMenu } = await import("../../app/menu");
    const [, actions] = vi.mocked(installAppMenu).mock.lastCall!;
    act(() => actions.newTab());
    expect(screen.getAllByRole("tab", { name: "Nueva pestaña" })).toHaveLength(
      1,
    );
    expect(store().activeId).toBeNull();
    // It is the last tab for ⌘1…⌘9 and ⌃Tab.
    act(() => store().activateIndex(0));
    act(() => store().activateIndex(2));
    expect(store().activeId).toBeNull();
    act(() => store().activateNext(1));
    expect(store().activeId).toBe(1);
    act(() => store().activateNext(-1));
    expect(store().activeId).toBeNull();
  });

  it("becomes the tab of the repository opened from it", async () => {
    const user = userEvent.setup();
    withTabs(1);
    ipc.listRecentRepositories.mockResolvedValue({
      status: "ok",
      data: [{ path: "/repos/nuevo", name: "nuevo", lastOpened: 1 }],
    });
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(7, "/repos/nuevo"),
    });
    render(<App />);
    act(() => store().newTab());
    await user.click(await screen.findByRole("button", { name: /^nuevo/ }));
    expect(ids()).toEqual([1, 7]);
    expect(store().activeId).toBe(7);
    expect(store().blankTab).toBe(false);
    expect(tab("nuevo")).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("tab", { name: "Nueva pestaña" })).toBeNull();
  });

  it("goes to the existing tab when that repository is already open", async () => {
    withTabs(2, 2);
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(1, "/repos/r1"),
    });
    store().newTab();
    await act(() => store().openRepository("/repos/r1"));
    expect(ids()).toEqual([1, 2]);
    expect(store().activeId).toBe(1);
    expect(store().blankTab).toBe(false);
  });

  it("also becomes the tab of a repository cloned or initialized from it", async () => {
    withTabs(1);
    store().newTab();
    await act(() => store().adopt(opened(9, "/repos/clonado")));
    expect(ids()).toEqual([1, 9]);
    expect(store().activeId).toBe(9);
    expect(store().blankTab).toBe(false);
  });

  it("keeps an inactive new tab when a repository opens from another tab", async () => {
    withTabs(1);
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(3, "/repos/r3"),
    });
    store().newTab();
    store().activate(1);
    await act(() => store().openRepository("/repos/r3"));
    expect(store().blankTab).toBe(true);
    expect(store().activeId).toBe(3);
  });

  it("closes with × or ⌘W, back to its left neighbour", async () => {
    const user = userEvent.setup();
    withTabs(2, 1);
    render(<App />);
    act(() => store().newTab());
    await user.click(
      screen.getByRole("button", { name: "Cerrar Nueva pestaña" }),
    );
    expect(store().blankTab).toBe(false);
    expect(store().activeId).toBe(2);

    act(() => store().newTab());
    await act(() => store().closeActiveTab());
    expect(store().blankTab).toBe(false);
    expect(ids()).toEqual([1, 2]);
  });

  it("is not saved in the session", async () => {
    useRepoStore.setState({
      tabs: [opened(1, "/a"), opened(2, "/b")],
      activeId: 1,
    });
    await store().restoreSession();
    store().newTab();
    expect(ipc.saveSession).toHaveBeenLastCalledWith(["/a", "/b"], "/b");
  });
});

function Scroller({ id }: { id: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useRememberedScroll(ref, id);
  return <div ref={ref} data-testid="scroller" />;
}

describe("view state per tab", () => {
  it("brings each tab's scroll position back", () => {
    // jsdom has no layout: a settable scrollTop is enough here.
    let value = 0;
    Object.defineProperty(HTMLElement.prototype, "scrollTop", {
      configurable: true,
      get: () => value,
      set: (v: number) => {
        value = v;
      },
    });
    const { rerender } = render(<Scroller id="1:graph" />);
    const scroller = screen.getByTestId("scroller");
    act(() => {
      scroller.dispatchEvent(new Event("wheel"));
      value = 420;
      scroller.dispatchEvent(new Event("scroll"));
    });

    rerender(<Scroller id="2:graph" />);
    expect(value).toBe(0);
    rerender(<Scroller id="1:graph" />);
    expect(value).toBe(420);

    forgetView(1);
    rerender(<Scroller id="2:graph" />);
    rerender(<Scroller id="1:graph" />);
    expect(value).toBe(0);
    delete (HTMLElement.prototype as { scrollTop?: number }).scrollTop;
  });
});
