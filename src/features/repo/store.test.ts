import { commandError, ipc, opened } from "../../test/ipc";
import { useRepoStore } from "./store";

const store = () => useRepoStore.getState();

describe("repo store", () => {
  it("opens a repository in a new active tab and refreshes recents", async () => {
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(1, "/src/app"),
    });
    ipc.listRecentRepositories.mockResolvedValue({
      status: "ok",
      data: [{ path: "/src/app", name: "app", lastOpened: 1 }],
    });

    await store().openRepository("/src/app/sub");

    expect(ipc.openRepository).toHaveBeenCalledWith("/src/app/sub");
    expect(store().tabs).toEqual([opened(1, "/src/app")]);
    expect(store().activeId).toBe(1);
    expect(store().opening).toBe(false);
    expect(store().recents).toHaveLength(1);
  });

  it("focuses the existing tab when the backend returns a known session", async () => {
    ipc.openRepository
      .mockResolvedValueOnce({ status: "ok", data: opened(1, "/a") })
      .mockResolvedValueOnce({ status: "ok", data: opened(2, "/b") })
      .mockResolvedValueOnce({ status: "ok", data: opened(1, "/a") });

    await store().openRepository("/a");
    await store().openRepository("/b");
    await store().openRepository("/a");

    expect(store().tabs.map((t) => t.id)).toEqual([1, 2]);
    expect(store().activeId).toBe(1);
  });

  it("keeps the failed path and error, and refreshes recents", async () => {
    ipc.openRepository.mockResolvedValue({
      status: "error",
      error: commandError("notARepository", "$ git rev-parse"),
    });

    await store().openRepository("/tmp/plain");

    expect(store().tabs).toEqual([]);
    expect(store().openError).toEqual({
      path: "/tmp/plain",
      error: commandError("notARepository", "$ git rev-parse"),
    });
    expect(ipc.listRecentRepositories).toHaveBeenCalled();

    store().dismissError();
    expect(store().openError).toBeNull();
  });

  it("does nothing when the folder picker is cancelled", async () => {
    ipc.pickFolder.mockResolvedValue(null);
    await store().pickAndOpen("Open");
    expect(ipc.pickFolder).toHaveBeenCalledWith("Open");
    expect(ipc.openRepository).not.toHaveBeenCalled();
  });

  it("opens the picked folder", async () => {
    ipc.pickFolder.mockResolvedValue("/picked");
    ipc.openRepository.mockResolvedValue({
      status: "ok",
      data: opened(3, "/picked"),
    });
    await store().pickAndOpen("Open");
    expect(store().activeId).toBe(3);
  });

  it("closing the active tab selects a neighbour and frees the session", async () => {
    useRepoStore.setState({
      tabs: [opened(1, "/a"), opened(2, "/b"), opened(3, "/c")],
      activeId: 2,
    });

    await store().closeTab(2);
    expect(store().activeId).toBe(3);
    expect(ipc.closeRepository).toHaveBeenCalledWith(2);

    await store().closeTab(3);
    expect(store().activeId).toBe(1);

    await store().closeTab(1);
    expect(store().activeId).toBeNull();
    expect(store().tabs).toEqual([]);
  });

  it("closing an inactive tab keeps the selection", async () => {
    useRepoStore.setState({
      tabs: [opened(1, "/a"), opened(2, "/b")],
      activeId: 1,
    });
    await store().closeTab(2);
    expect(store().activeId).toBe(1);
  });
});
