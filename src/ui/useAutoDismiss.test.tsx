import { act, fireEvent, render, screen } from "@testing-library/react";
import i18n from "../i18n";
import { ipc } from "../test/ipc";
import { HistoryToast } from "../features/history/HistoryDialogs";
import { useHistoryStore } from "../features/history/store";
import { RemoteToast } from "../features/remote/RemoteStatus";
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
