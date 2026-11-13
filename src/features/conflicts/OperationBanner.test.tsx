import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { ConflictSides, InProgress, WorkingStatus } from "../../ipc";
import { cleanStatus, commandError, ipc } from "../../test/ipc";
import { WorkingDirPanel } from "../workdir/WorkingDirPanel";
import { useWorkdirStore } from "../workdir/store";
import { InProgressBanner } from "./OperationBanner";
import { useConflictStore } from "./store";

const commit = (short: string, subject: string) => ({
  oid: short.padEnd(40, "0"),
  short,
  subject,
});

const rebase: ConflictSides = {
  type: "rebase",
  branch: "topic",
  onto: { commit: commit("ccccccc", "main change"), branches: ["main"] },
  head: commit("ccccccc", "main change"),
  applying: commit("ddddddd", "topic two"),
  progress: { current: 2, total: 3 },
};

const merge: ConflictSides = {
  type: "merge",
  branch: "main",
  head: commit("aaaaaaa", "main change"),
  merging: { commit: commit("bbbbbbb", "feature"), branches: ["feature"] },
};

function stopped(operation: InProgress, conflicts: number): WorkingStatus {
  return {
    ...cleanStatus(),
    inProgress: operation,
    conflicted: Array.from({ length: conflicts }, (_, i) => ({
      path: `f${i}.txt`,
      kind: "bothModified" as const,
    })),
  };
}

function show(operation: InProgress, sides: ConflictSides, conflicts: number) {
  ipc.conflictSides.mockResolvedValue({ status: "ok", data: sides });
  // Re-reading the status after an action finds the operation still there.
  ipc.workingStatus.mockResolvedValue({
    status: "ok",
    data: stopped(operation, conflicts),
  });
  useWorkdirStore.setState({
    statuses: { 1: { status: stopped(operation, conflicts), error: null } },
  });
  render(<InProgressBanner repoId={1} />);
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
});

describe("the banner of an operation in progress", () => {
  it("names a rebase, its progress and the files left", async () => {
    show("rebase", rebase, 2);
    const banner = screen.getByRole("status");
    expect(
      await within(banner).findByText(/Rebase de «topic» sobre «main»/),
    ).toBeInTheDocument();
    expect(banner).toHaveTextContent("commit 2 de 3: «topic two»");
    expect(banner).toHaveTextContent("Quedan 2 archivos con conflictos.");
    expect(
      within(banner).getByRole("button", { name: "Continuar" }),
    ).toBeDisabled();
  });

  it("continues a rebase once nothing is left in conflict", async () => {
    const user = userEvent.setup();
    ipc.continueInProgress.mockResolvedValue({
      status: "ok",
      data: { type: "stoppedAgain", conflicts: ["f.txt"] },
    });
    show("rebase", rebase, 0);
    await screen.findByText(/Rebase de «topic»/);
    expect(screen.getByRole("status")).toHaveTextContent(
      "«Continuar» aplica este commit",
    );
    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(ipc.continueInProgress).toHaveBeenCalledWith(1, null);
  });

  it("skips the commit only after a confirmation that names it", async () => {
    const user = userEvent.setup();
    ipc.skipCurrent.mockResolvedValue({ status: "ok", data: { type: "done" } });
    show("rebase", rebase, 1);
    await screen.findByText(/Rebase de «topic»/);
    await user.click(
      screen.getByRole("button", { name: "Saltar este commit…" }),
    );
    const dialog = screen.getByRole("alertdialog", {
      name: "¿Saltar este commit?",
    });
    expect(dialog).toHaveTextContent("Se salta ddddddd «topic two»");
    expect(ipc.skipCurrent).not.toHaveBeenCalled();
    await user.click(
      within(dialog).getByRole("button", { name: "Saltar este commit" }),
    );
    expect(ipc.skipCurrent).toHaveBeenCalledWith(1);
  });

  it("offers to skip a commit that ends up empty", async () => {
    const user = userEvent.setup();
    ipc.continueInProgress.mockResolvedValue({
      status: "ok",
      data: { type: "nothingToCommit" },
    });
    ipc.skipCurrent.mockResolvedValue({ status: "ok", data: { type: "done" } });
    show("rebase", rebase, 0);
    await screen.findByText(/Rebase de «topic»/);
    await user.click(screen.getByRole("button", { name: "Continuar" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "Este commit ya no cambia nada",
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Saltar este commit" }),
    );
    expect(ipc.skipCurrent).toHaveBeenCalledWith(1);
  });

  it("concludes a merge with the commit box's message", async () => {
    const user = userEvent.setup();
    ipc.continueInProgress.mockResolvedValue({
      status: "ok",
      data: { type: "done" },
    });
    useWorkdirStore.setState({
      drafts: {
        1: { summary: "Join feature", description: "Kept both.", amend: false },
      },
    });
    show("merge", merge, 0);
    expect(
      await screen.findByText(/Merge de «feature» en «main»/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Saltar este commit…" }),
    ).toBeNull();
    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(ipc.continueInProgress).toHaveBeenCalledWith(
      1,
      "Join feature\n\nKept both.\n",
    );
  });

  it("asks before continuing with unsaved choices", async () => {
    const user = userEvent.setup();
    ipc.continueInProgress.mockResolvedValue({
      status: "ok",
      data: { type: "done" },
    });
    useConflictStore
      .getState()
      .setChoice(1, "f0.txt", "fp", 0, { kind: "ours" });
    show("merge", merge, 0);
    await screen.findByText(/Merge de «feature»/);
    await user.click(screen.getByRole("button", { name: "Continuar" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "Hay elecciones sin guardar",
    });
    expect(dialog).toHaveTextContent("f0.txt");
    expect(ipc.continueInProgress).not.toHaveBeenCalled();
    await user.click(
      within(dialog).getByRole("button", { name: "Continuar sin guardarlas" }),
    );
    expect(ipc.continueInProgress).toHaveBeenCalled();
    expect(useConflictStore.getState().drafts[1]).toBeUndefined();
  });

  it("shows why it could not continue (a hook, for instance)", async () => {
    const user = userEvent.setup();
    ipc.continueInProgress.mockResolvedValue({
      status: "error",
      error: commandError("unknown", "$ git commit\nhook says no"),
    });
    show("merge", merge, 0);
    await screen.findByText(/Merge de «feature»/);
    await user.click(screen.getByRole("button", { name: "Continuar" }));
    expect(
      await screen.findByText(/No se ha podido continuar/),
    ).toBeInTheDocument();
  });
});

describe("the commit box during a merge", () => {
  it("starts from git's message", async () => {
    ipc.preparedMessage.mockResolvedValue({
      status: "ok",
      data: "Merge branch 'feature'\n",
    });
    useWorkdirStore.setState({
      statuses: { 1: { status: stopped("merge", 0), error: null } },
    });
    render(<WorkingDirPanel repoId={1} />);
    expect(
      await screen.findByDisplayValue("Merge branch 'feature'"),
    ).toBeInTheDocument();
  });
});
