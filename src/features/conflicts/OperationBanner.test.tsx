import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { ConflictSides, InProgress, WorkingStatus } from "../../ipc";
import { cleanStatus, commandError, ipc } from "../../test/ipc";
import { WorkingDirPanel } from "../workdir/WorkingDirPanel";
import { useWorkdirStore } from "../workdir/store";
import { AbortDialog } from "../remote/RemoteDialogs";
import { RemoteToast } from "../remote/RemoteStatus";
import { InProgressBanner } from "./OperationBanner";
import { originOf, rememberOrigin } from "./origin";
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
  interactive: false,
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

describe("each operation with its own texts", () => {
  /** The banner, the abort dialog and the notice after aborting. */
  async function abortIt(name: RegExp, title: string) {
    const user = userEvent.setup();
    ipc.abortInProgress.mockResolvedValue({ status: "ok", data: null });
    render(
      <>
        <AbortDialog />
        <RemoteToast repoId={1} />
      </>,
    );
    await screen.findByText(name);
    await user.click(screen.getByRole("button", { name: "Abortar…" }));
    const dialog = screen.getByRole("alertdialog", { name: title });
    await user.click(within(dialog).getByRole("button", { name: "Abortar" }));
    expect(ipc.abortInProgress).toHaveBeenCalledWith(1);
  }

  it("GitCRAFT's interactive rebase: title, Continue, Skip and Abort", async () => {
    const user = userEvent.setup();
    show("rebase", { ...rebase, interactive: true }, 0);
    const banner = screen.getByRole("status");
    expect(
      await within(banner).findByText(/Rebase interactivo de «topic»/),
    ).toBeInTheDocument();
    expect(banner).toHaveTextContent(
      "«Continuar» aplica este commit, con su mensaje nuevo si se lo diste, y sigue con el resto del rebase interactivo.",
    );
    await user.click(
      screen.getByRole("button", { name: "Saltar este commit…" }),
    );
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "ni el mensaje nuevo que le hubieras dado. Si le combinabas otros commits, siguen en el rebase",
    );
    await user.click(screen.getByRole("button", { name: "Cancelar" }));
    await abortIt(
      /Rebase interactivo de «topic»/,
      "¿Abortar el rebase interactivo?",
    );
    expect(
      await screen.findByText(
        "Rebase interactivo abortado: todo está como antes del rebase interactivo.",
      ),
    ).toBeInTheDocument();
  });

  it("a pull with rebase that GitCRAFT started", async () => {
    rememberOrigin(1, "pullRebase");
    show("rebase", rebase, 1);
    expect(
      await screen.findByText(/Pull de «main» en «topic», con rebase/),
    ).toBeInTheDocument();
    await abortIt(/Pull de «main»/, "¿Abortar el pull?");
    expect(
      await screen.findByText(
        "Pull abortado: el rebase se ha deshecho y todo está como antes del pull.",
      ),
    ).toBeInTheDocument();
  });

  it("a rebase GitCRAFT did not start never mentions a pull", async () => {
    show("rebase", rebase, 1);
    await abortIt(/Rebase de «topic» sobre «main»/, "¿Abortar el rebase?");
    expect(
      await screen.findByText(
        "Rebase abortado: todo está como antes del rebase.",
      ),
    ).toBeInTheDocument();
  });

  it("forgets the origin once nothing is in progress", () => {
    rememberOrigin(1, "pullMerge");
    const status = (inProgress: InProgress | null) =>
      useWorkdirStore.setState({
        statuses: {
          1: { status: { ...cleanStatus(), inProgress }, error: null },
        },
      });
    // A status read before the pull stopped does not count.
    status(null);
    expect(originOf(1, "merge", merge)).toBe("pullMerge");
    status("merge");
    expect(originOf(1, "merge", merge)).toBe("pullMerge");
    // Another kind of operation is not the pull.
    expect(originOf(1, "rebase", rebase)).toBeNull();
    status(null);
    expect(originOf(1, "merge", merge)).toBeNull();
  });
});
