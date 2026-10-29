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
