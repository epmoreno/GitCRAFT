import { act, fireEvent, render, screen, within } from "@testing-library/react";
import i18n from "../i18n";
import { commandError, ipc, opened } from "../test/ipc";
import { TabBar } from "../features/repo/TabBar";
import { useRepoStore } from "../features/repo/store";
import { HistoryToast } from "../features/history/HistoryDialogs";
import { useHistoryStore } from "../features/history/store";
import { RemoteProgress, RemoteToast } from "../features/remote/RemoteStatus";
import { useRemoteStore } from "../features/remote/store";
import { NOTICE_SECONDS } from "./useAutoDismiss";

const second = () => act(() => vi.advanceTimersByTime(1000));
const seconds = (n: number) => act(() => vi.advanceTimersByTime(n * 1000));

function rebased(repoId: number) {
  useHistoryStore.setState({
    notice: {
      kind: "merged",
      op: "interactiveRebase",
      repoId,
      undoId: 7,
      count: 2,
      published: false,
      source: "main",
      target: "main",
      fastForward: false,
      undoGone: false,
    },
  });
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  ipc.undoAvailable.mockResolvedValue({ status: "ok", data: true });
  vi.useFakeTimers();
});

afterEach(() => vi.useRealTimers());

describe("notices", () => {
  it(`close by themselves after ${NOTICE_SECONDS} seconds, with «Deshacer» too`, () => {
    expect(NOTICE_SECONDS).toBe(25);
    rebased(1);
    render(<HistoryToast repoId={1} />);
    expect(screen.getByRole("button", { name: "Deshacer" })).toBeVisible();
    seconds(NOTICE_SECONDS - 1);
    expect(screen.getByRole("status")).toBeVisible();
    second();
    expect(screen.queryByRole("status")).toBeNull();
    expect(useHistoryStore.getState().notice).toBeNull();
  });

  it("wait while the pointer is over them, then go on counting", () => {
    rebased(1);
    render(<HistoryToast repoId={1} />);
    seconds(20);
    fireEvent.mouseEnter(screen.getByRole("status"));
    seconds(60);
    expect(screen.getByRole("status")).toBeVisible();
    fireEvent.mouseLeave(screen.getByRole("status"));
    // The 5 seconds that were left, not 25 again.
    seconds(4);
    expect(screen.getByRole("status")).toBeVisible();
    second();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("also wait while the keyboard focus is inside them", () => {
    rebased(1);
    render(<HistoryToast repoId={1} />);
    act(() => screen.getByRole("button", { name: "Deshacer" }).focus());
    seconds(60);
    expect(screen.getByRole("status")).toBeVisible();
    act(() => screen.getByRole("button", { name: "Deshacer" }).blur());
    seconds(NOTICE_SECONDS);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("show only in the tab of their repository", () => {
    useRemoteStore.setState({
      notice: {
        kind: "aborted",
        repoId: 1,
        operation: "rebase",
        origin: "interactiveRebase",
      },
    });
    const { rerender } = render(<RemoteToast repoId={2} />);
    expect(screen.queryByRole("status")).toBeNull();
    // Hidden, it does not use up its time.
    seconds(NOTICE_SECONDS * 2);
    rerender(<RemoteToast repoId={1} />);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Rebase interactivo abortado",
    );
    seconds(NOTICE_SECONDS);
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("the user's decisions on notices", () => {
  it("an error stays until it is closed", () => {
    useRemoteStore.setState({
      notice: {
        kind: "error",
        repoId: 1,
        operation: "push",
        error: commandError("authFailed"),
      },
    });
    render(<RemoteToast repoId={1} />);
    seconds(NOTICE_SECONDS * 10);
    expect(screen.getByText(/No se pudo hacer push/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(useRemoteStore.getState().notice).toBeNull();
  });

  it("«Recuperar mis cambios» says where else they are, and closes", () => {
    useHistoryStore.setState({
      saved: {
        1: { oid: "a".repeat(40), pick: "interactiveRebase", short: "main" },
      },
    });
    render(<HistoryToast repoId={1} />);
    expect(screen.getByRole("status")).toHaveTextContent(
      "También puedes recuperarlos desde Stashes, en la barra lateral.",
    );
    seconds(NOTICE_SECONDS);
    expect(screen.queryByRole("status")).toBeNull();
    // Only the offer goes: the stash entry is untouched.
    expect(ipc.dropStash).not.toHaveBeenCalled();
  });

  it("the progress of a fetch shows in its tab; the others mark that tab", () => {
    useRepoStore.setState({
      tabs: [opened(1, "/r/web"), opened(2, "/r/api")],
      activeId: 2,
    });
    useRemoteStore.setState({
      running: {
        repoId: 1,
        kind: "fetch",
        remote: "origin",
        line: "",
        percent: null,
      },
    });
    const { rerender } = render(
      <>
        <TabBar />
        <RemoteProgress repoId={2} />
      </>,
    );
    expect(screen.queryByRole("progressbar")).toBeNull();
    const web = screen.getByRole("tab", { name: /web/ });
    expect(
      within(web).getByRole("img", { name: "Descargando de origin…" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /api/ })).not.toContainElement(
      screen.queryByRole("img"),
    );
    act(() => useRepoStore.setState({ activeId: 1 }));
    rerender(
      <>
        <TabBar />
        <RemoteProgress repoId={1} />
      </>,
    );
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /Descargando/ })).toBeNull();
  });
});
