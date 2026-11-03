import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import {
  commandError,
  defaultGeneralSettings,
  emptyRefs,
  ipc,
  opened,
} from "../../test/ipc";
import { CurrentBranch } from "../remote/RemoteButtons";
import { fetchOpenRepositories, useAutoFetch } from "../remote/autoFetch";
import { useRemoteStore } from "../remote/store";
import { useRepoStore } from "../repo/store";
import { SettingsDialog } from "./SettingsDialog";
import { useSettingsStore } from "./store";

const defaults = (overrides = {}) => ({
  defaultBranch: null,
  pullStrategy: null,
  pullRebase: null,
  pullFf: null,
  ...overrides,
});

async function openGeneral() {
  render(<SettingsDialog />);
  act(() => useSettingsStore.getState().openSettings("general"));
  return screen.findByRole("heading", { name: "Git (configuración global)" });
}

describe("git defaults in Settings → General", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("says that they are global and also apply in the terminal", async () => {
    await openGeneral();
    expect(
      screen.getByText(/también afectan a git en la terminal/),
    ).toBeVisible();
    expect(
      screen.getByText(/en la terminal, git usa «master».*se propone «main»/),
    ).toBeVisible();
  });

  it("shows the exact command and the previous value before writing the default branch", async () => {
    const user = userEvent.setup();
    ipc.gitDefaults.mockResolvedValue({
      status: "ok",
      data: defaults({ defaultBranch: "master" }),
    });
    ipc.setDefaultBranch.mockResolvedValue({
      status: "ok",
      data: defaults({ defaultBranch: "trunk" }),
    });
    await openGeneral();
    const input = screen.getByRole("textbox", {
      name: "Rama inicial por defecto",
    });
    await user.clear(input);
    await user.type(input, "trunk");
    await user.click(screen.getByRole("button", { name: "Guardar" }));

    const confirm = screen.getByRole("alertdialog", {
      name: "¿Cambiar la configuración global de git?",
    });
    expect(confirm).toHaveTextContent(
      "git config --global init.defaultBranch trunk",
    );
    expect(confirm).toHaveTextContent("Valor anterior: master");
    expect(confirm).toHaveTextContent("también a git en la terminal");
    expect(ipc.setDefaultBranch).not.toHaveBeenCalled();

    // Escape closes the confirmation only, not Settings.
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Ajustes" })).toBeVisible();
    expect(ipc.setDefaultBranch).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await user.click(screen.getByRole("button", { name: "Cambiar" }));
    expect(ipc.setDefaultBranch).toHaveBeenCalledWith("trunk");
    expect(input).toHaveValue("trunk");
  });

  it("explains an invalid branch name", async () => {
    const user = userEvent.setup();
    ipc.setDefaultBranch.mockResolvedValue({
      status: "error",
      error: commandError("invalidBranchName"),
    });
    await openGeneral();
    await user.type(
      screen.getByRole("textbox", { name: "Rama inicial por defecto" }),
      "a..b",
    );
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    await user.click(screen.getByRole("button", { name: "Cambiar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ese nombre no es válido para una rama.",
    );
  });

  it("lists what changes for the pull strategy, with the values it replaces", async () => {
    const user = userEvent.setup();
    ipc.gitDefaults.mockResolvedValue({
      status: "ok",
      data: defaults({ pullStrategy: "rebase", pullRebase: "true" }),
    });
    ipc.pullStrategyChanges.mockResolvedValue({
      status: "ok",
      data: [
        { key: "pull.ff", value: "only" },
        { key: "pull.rebase", value: null },
      ],
    });
    ipc.setDefaultPullStrategy.mockResolvedValue({
      status: "ok",
      data: defaults({ pullStrategy: "fastForwardOnly", pullFf: "only" }),
    });
    await openGeneral();
    expect(screen.getByRole("radio", { name: "Rebase" })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: "Solo fast-forward" }));
    expect(ipc.pullStrategyChanges).toHaveBeenCalledWith("fastForwardOnly");

    const confirm = screen.getByRole("alertdialog");
    const rows = within(confirm).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("git config --global pull.ff only");
    expect(rows[0]).toHaveTextContent("Valor anterior: sin configurar");
    expect(rows[1]).toHaveTextContent(
      "git config --global --unset-all pull.rebase",
    );
    expect(rows[1]).toHaveTextContent("Valor anterior: true");
    // Nothing changes until confirmed.
    expect(screen.getByRole("radio", { name: "Rebase" })).toBeChecked();

    await user.click(within(confirm).getByRole("button", { name: "Cambiar" }));
    expect(ipc.setDefaultPullStrategy).toHaveBeenCalledWith("fastForwardOnly");
    expect(
      screen.getByRole("radio", { name: "Solo fast-forward" }),
    ).toBeChecked();
  });
});

describe("automatic fetch", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    useRepoStore.setState({
      tabs: [opened(1, "/r/con-remoto"), opened(2, "/r/local")],
      activeId: 1,
    });
    ipc.getRefs.mockImplementation(async (id) => ({
      status: "ok",
      data: { ...emptyRefs(), remotes: id === 1 ? ["origin"] : [] },
    }));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("is off by default and saves the interval", async () => {
    const user = userEvent.setup();
    await openGeneral();
    const select = screen.getByRole("combobox", { name: "Fetch automático" });
    expect(select).toHaveValue("0");
    await user.selectOptions(select, "15");
    expect(ipc.setGeneralSettings).toHaveBeenLastCalledWith({
      ...defaultGeneralSettings(),
      autoFetchMinutes: 15,
    });
  });

  it("fetches the open repositories with a remote, every N minutes", async () => {
    vi.useFakeTimers();
    function Probe() {
      useAutoFetch();
      return null;
    }
    render(<Probe />);
    await act(async () => vi.advanceTimersByTime(60 * 60_000));
    expect(ipc.fetchRemote).not.toHaveBeenCalled();

    act(() =>
      useSettingsStore.setState({
        general: { ...defaultGeneralSettings(), autoFetchMinutes: 5 },
      }),
    );
    await act(async () => vi.advanceTimersByTime(5 * 60_000));
    expect(ipc.fetchRemote).toHaveBeenCalledTimes(1);
    expect(ipc.fetchRemote.mock.calls[0]?.slice(0, 3)).toEqual([
      1,
      null,
      false,
    ]);
  });

  it("skips repositories while another remote operation runs", async () => {
    useRemoteStore.setState({
      running: {
        repoId: 1,
        kind: "push",
        remote: null,
        line: "",
        percent: null,
      },
    });
    await fetchOpenRepositories();
    expect(ipc.fetchRemote).not.toHaveBeenCalled();
  });

  it("marks a failure next to the branch, without a toast or a dialog", async () => {
    ipc.fetchRemote.mockResolvedValue({
      status: "error",
      error: commandError("remoteUnreachable"),
    });
    await act(() => fetchOpenRepositories());
    expect(useRemoteStore.getState().notice).toBeNull();
    expect(useRemoteStore.getState().dialog).toBeNull();
    expect(useRemoteStore.getState().autoFetchErrors[1]?.kind).toBe(
      "remoteUnreachable",
    );

    const { useGraphStore } = await import("../graph/store");
    useGraphStore.setState({
      graphs: {
        1: {
          status: "done",
          lines: [],
          maxColumn: 0,
          refs: { ...emptyRefs(), remotes: ["origin"] },
          error: null,
          selectedOid: null,
          suggestOptimize: false,
          openFile: null,
          loadId: 1,
        },
      },
    });
    render(<CurrentBranch repoId={1} />);
    const mark = screen.getByText(/fetch falló/);
    expect(mark.getAttribute("title")).toMatch(
      /^El último fetch automático falló: /,
    );

    // The next fetch that works clears it.
    ipc.fetchRemote.mockResolvedValue({ status: "ok", data: null });
    await act(() => fetchOpenRepositories());
    expect(screen.queryByText(/fetch falló/)).not.toBeInTheDocument();
  });
});
