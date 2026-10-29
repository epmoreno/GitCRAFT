import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { CommandRecord } from "../../ipc";
import { ipc, opened } from "../../test/ipc";
import { Toolbar } from "../../app/Toolbar";
import { useRemoteStore } from "../remote/store";
import { useRepoStore } from "../repo/store";
import { formatDuration, OperationsPanel } from "./OperationsPanel";
import { useOperationsStore } from "./store";

const record = (
  id: number,
  overrides: Partial<CommandRecord> = {},
): CommandRecord => ({
  id,
  cwd: "/r/uno",
  command: `git -c core.quotepath=false -c color.ui=false status ${id}`,
  write: false,
  startedAt: Date.UTC(2026, 0, 1, 10, 0, id),
  durationMs: 12,
  outcome: { type: "succeeded" },
  ...overrides,
});

function renderPanel() {
  useRepoStore.setState({
    tabs: [opened(1, "/r/uno"), opened(2, "/r/dos")],
    activeId: 1,
  });
  render(
    <>
      <Toolbar />
      <OperationsPanel />
    </>,
  );
}

const panel = () => screen.getByRole("region", { name: "Operaciones" });
const history = () =>
  within(panel()).getByRole("list", { name: "Historial de comandos" });

describe("operations panel", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("opens from the toolbar and lists the exact commands, newest first", async () => {
    const user = userEvent.setup();
    ipc.commandHistory.mockResolvedValue({
      status: "ok",
      data: [
        record(1),
        record(2, {
          command:
            "git -c core.quotepath=false -c color.ui=false push --porcelain -- https://***@example.com/r.git main",
          write: true,
          durationMs: 1450,
          outcome: { type: "failed", code: 128 },
        }),
      ],
    });
    await act(() => useOperationsStore.getState().load());
    renderPanel();
    expect(screen.queryByRole("region")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Operaciones" }));

    const rows = within(history()).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent(
      "push --porcelain -- https://***@example.com/r.git main",
    );
    expect(rows[0]).toHaveTextContent("escritura");
    expect(rows[0]).toHaveTextContent("1,5s");
    expect(rows[0]).toHaveTextContent("Error (código 128)");
    expect(rows[1]).toHaveTextContent(
      "git -c core.quotepath=false -c color.ui=false status 1",
    );
    expect(rows[1]).toHaveTextContent("12ms");
    expect(rows[1]).toHaveTextContent("Correcto");
    expect(
      within(rows[0] as HTMLElement).getByText(/push --porcelain/),
    ).toHaveClass("selectable");
  });

  it("filters by tab and hides reads", async () => {
    const user = userEvent.setup();
    useOperationsStore.setState({
      open: true,
      records: [
        record(1),
        record(2, { cwd: "/r/dos", command: "git fetch dos" }),
        record(3, { write: true, command: "git commit uno" }),
        record(4, { cwd: "/r", command: "git clone -- x uno" }),
      ],
    });
    renderPanel();
    const texts = () =>
      within(history())
        .getAllByRole("listitem")
        .map((li) => li.querySelector("code")?.textContent);
    expect(texts()).toEqual(["git commit uno", record(1).command]);

    await user.click(
      within(panel()).getByRole("checkbox", { name: "Solo esta pestaña" }),
    );
    expect(texts()).toEqual([
      "git clone -- x uno",
      "git commit uno",
      "git fetch dos",
      record(1).command,
    ]);
    // Without the tab filter each row says where it ran.
    expect(within(history()).getAllByRole("listitem")[2]).toHaveTextContent(
      "dos",
    );

    await user.click(
      within(panel()).getByRole("checkbox", { name: "Ocultar lecturas" }),
    );
    expect(texts()).toEqual(["git commit uno"]);
  });

  it("shows running commands as they arrive and when they end", async () => {
    useOperationsStore.setState({ open: true });
    renderPanel();
    expect(
      within(panel()).getByText("No hay ninguna operación en curso."),
    ).toBeVisible();

    const running = record(5, {
      command: "git fetch --progress origin",
      startedAt: Date.now(),
      durationMs: null,
      outcome: null,
    });
    act(() => useOperationsStore.getState().receive(running));
    const list = within(panel()).getByRole("list", { name: "En curso" });
    expect(list).toHaveTextContent("git fetch --progress origin");
    expect(list).toHaveTextContent("En curso");
    expect(
      screen.getByRole("button", { name: /Operaciones/ }),
    ).toHaveTextContent("●");

    act(() =>
      useOperationsStore.getState().receive({
        ...running,
        durationMs: 300,
        outcome: { type: "cancelled" },
      }),
    );
    expect(
      within(panel()).queryByRole("list", { name: "En curso" }),
    ).not.toBeInTheDocument();
    expect(history()).toHaveTextContent("Cancelado");
  });

  it("shows the remote operation with its progress and cancels it", async () => {
    const user = userEvent.setup();
    useOperationsStore.setState({ open: true });
    useRemoteStore.setState({
      running: {
        repoId: 1,
        kind: "push",
        remote: null,
        line: "Writing objects:  40% (2/5)",
        percent: 40,
      },
    });
    renderPanel();
    const status = within(panel()).getByRole("status");
    expect(within(status).getByRole("progressbar")).toHaveAttribute(
      "value",
      "40",
    );
    await user.click(within(status).getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelOperation).toHaveBeenCalledWith(1);
  });

  it("clears the finished history", async () => {
    const user = userEvent.setup();
    useOperationsStore.setState({
      open: true,
      records: [record(1), record(2, { outcome: null, durationMs: null })],
    });
    renderPanel();
    await user.click(
      within(panel()).getByRole("button", { name: "Limpiar historial" }),
    );
    expect(ipc.clearCommandHistory).toHaveBeenCalled();
    expect(useOperationsStore.getState().records.map((r) => r.id)).toEqual([2]);
  });

  it("formats durations in the UI language", () => {
    expect(formatDuration(5, "es")).toBe("5ms");
    expect(formatDuration(2345, "es")).toBe("2,3s");
    expect(formatDuration(2345, "en")).toBe("2.3s");
    expect(formatDuration(125_000, "en")).toBe("2m 5s");
  });
});
