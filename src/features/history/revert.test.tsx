import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { PickPreview } from "../../ipc";
import { cleanStatus, ipc } from "../../test/ipc";
import { useWorkdirStore } from "../workdir/store";
import { CommitActionButtons } from "./CommitActionButtons";
import { HistoryDialogs, HistoryToast } from "./HistoryDialogs";
import { useHistoryStore } from "./store";

const OID = "f00dcafe".padEnd(40, "0");
const STASH = "5".repeat(40);

function preview(overrides: Partial<PickPreview> = {}): PickPreview {
  return {
    kind: "revert",
    commit: { oid: OID, short: "f00dcaf", subject: "edit f" },
    branch: "main",
    refused: null,
    blocked: null,
    blocking: [],
    conflicts: [],
    exact: true,
    ...overrides,
  };
}

function show(p: PickPreview) {
  ipc.revertPreview.mockResolvedValue({ status: "ok", data: p });
  render(
    <>
      <HistoryDialogs />
      <HistoryToast repoId={1} />
    </>,
  );
  return act(() => useHistoryStore.getState().requestPick(1, OID, "revert"));
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
});

describe("revert", () => {
  it("is offered for a commit of the branch, in the panel", async () => {
    const user = userEvent.setup();
    ipc.revertPreview.mockResolvedValue({ status: "ok", data: preview() });
    render(
      <>
        <CommitActionButtons repoId={1} oid={OID} />
        <HistoryDialogs />
      </>,
    );
    // The default facts: in the branch, so cherry-pick is off and revert on.
    expect(
      await screen.findByRole("button", { name: "Cherry-pick…" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Revertir…" }));
    expect(ipc.revertPreview).toHaveBeenCalledWith(1, OID);
    expect(
      await screen.findByRole("alertdialog", {
        name: "¿Revertir f00dcaf en «main»?",
      }),
    ).toBeInTheDocument();
  });

  it("explains it adds a commit, then reverts and says so", async () => {
    const user = userEvent.setup();
    ipc.revert.mockResolvedValue({
      status: "ok",
      data: { type: "done", head: "9".repeat(40), saved: null },
    });
    await show(preview());
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      'Se añade a «main» un commit nuevo, «Revert "edit f"»',
    );
    expect(dialog).toHaveTextContent("El commit original no se borra");
    await user.click(within(dialog).getByRole("button", { name: "Revertir" }));
    expect(ipc.revert).toHaveBeenCalledWith(1, OID, false, false);
    expect(ipc.cherryPick).not.toHaveBeenCalled();
    expect(
      await screen.findByText(
        "Revertido: «edit f» se ha deshecho con un commit nuevo en «main».",
      ),
    ).toBeInTheDocument();
  });

  it("saves changes first, keeps them on abort, and offers them back", async () => {
    const user = userEvent.setup();
    await show(
      preview({
        blocked: "stagedChanges",
        blocking: [{ path: "other.txt", untracked: false }],
        conflicts: ["f.txt"],
      }),
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      "git no hace un revert con cambios preparados",
    );
    expect(dialog).toHaveTextContent("«GitCRAFT: antes de revertir f00dcaf»");
    expect(dialog).toHaveTextContent(
      "Revertir producirá conflictos en 1 archivo:",
    );
    expect(dialog).toHaveTextContent("no vuelven solos al abortar");
    ipc.revert.mockResolvedValue({
      status: "ok",
      data: { type: "done", head: "9".repeat(40), saved: STASH },
    });
    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y revertir con conflictos",
      }),
    );
    expect(ipc.revert).toHaveBeenCalledWith(1, OID, true, true);
    expect(
      await screen.findByText(
        "Tus cambios siguen guardados en el stash («GitCRAFT: antes de revertir f00dcaf»).",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeInTheDocument();
  });
});
