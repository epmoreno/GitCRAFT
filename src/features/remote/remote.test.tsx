import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { Toolbar } from "../../app/Toolbar";
import { commandError, ipc, opened, sampleRefs } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { useRepoStore } from "../repo/store";
import { Sidebar } from "../sidebar/Sidebar";
import { RemoteProgress, RemoteToast } from "./RemoteStatus";

function renderToolbar() {
  useRepoStore.setState({ tabs: [opened(1, "/src/sample")], activeId: 1 });
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
  ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
  render(
    <>
      <Toolbar />
      <Sidebar repoId={1} />
      <RemoteProgress />
      <RemoteToast />
    </>,
  );
}

describe("fetch", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("shows git's progress and can be cancelled", async () => {
    const user = userEvent.setup();
    let report: (line: string) => void = () => {};
    let finish: (
      value: Awaited<ReturnType<typeof ipc.fetchRemote>>,
    ) => void = () => {};
    ipc.fetchRemote.mockImplementation((_id, _remote, _prune, onProgress) => {
      report = onProgress;
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Fetch" }));
    expect(ipc.fetchRemote).toHaveBeenCalledWith(
      1,
      null,
      false,
      expect.any(Function),
    );

    act(() => report("Receiving objects:  45% (9/20)"));
    const progress = screen.getByRole("status");
    expect(progress).toHaveTextContent("Descargando de todos los remotos…");
    expect(within(progress).getByRole("progressbar")).toHaveAttribute(
      "value",
      "45",
    );
    expect(progress).toHaveTextContent("Receiving objects: 45% (9/20)");
    // One remote operation at a time.
    expect(screen.getByRole("button", { name: "Fetch" })).toBeDisabled();

    await user.click(
      within(progress).getByRole("button", { name: "Cancelar" }),
    );
    expect(ipc.cancelOperation).toHaveBeenCalledWith(1);
    await act(async () =>
      finish({ status: "error", error: commandError("cancelled") }),
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("fetches one remote with --prune from the menus", async () => {
    const user = userEvent.setup();
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Opciones de fetch" }));
    await user.click(
      screen.getByRole("menuitem", { name: /todos y quitar ramas remotas/ }),
    );
    expect(ipc.fetchRemote).toHaveBeenLastCalledWith(
      1,
      null,
      true,
      expect.any(Function),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Tus ramas y archivos no han cambiado",
    );

    await user.pointer({
      keys: "[MouseRight]",
      target: screen.getByText("origin"),
    });
    await user.click(
      screen.getByRole("menuitem", { name: /Fetch de «origin» y quitar/ }),
    );
    expect(ipc.fetchRemote).toHaveBeenLastCalledWith(
      1,
      "origin",
      true,
      expect.any(Function),
    );
  });

  it("explains authentication failures", async () => {
    const user = userEvent.setup();
    ipc.fetchRemote.mockResolvedValue({
      status: "error",
      error: commandError(
        "authFailed",
        "fatal: could not read Username for 'https://example.com': terminal prompts disabled",
      ),
    });
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Fetch" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("No se pudo hacer fetch.");
    expect(alert).toHaveTextContent(/credential helper/);
    expect(alert).toHaveTextContent(/terminal prompts disabled/);
  });
});
