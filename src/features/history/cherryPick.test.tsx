import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { PickPreview, StashEntry } from "../../ipc";
import { cleanStatus, ipc } from "../../test/ipc";
import { AbortDialog } from "../remote/RemoteDialogs";
import { useRemoteStore } from "../remote/store";
import { useWorkdirStore } from "../workdir/store";
import { CommitActionButtons } from "./CommitActionButtons";
import { HistoryDialogs, HistoryToast } from "./HistoryDialogs";
import { useHistoryStore } from "./store";

const OID = "a1b2c3d".padEnd(40, "0");
const STASH = "5".repeat(40);

function preview(overrides: Partial<PickPreview> = {}): PickPreview {
  return {
    commit: { oid: OID, short: "a1b2c3d", subject: "topic edits f" },
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
  ipc.cherryPickPreview.mockResolvedValue({ status: "ok", data: p });
  render(
    <>
      <HistoryDialogs />
      <HistoryToast repoId={1} />
    </>,
  );
  return act(() => useHistoryStore.getState().requestCherryPick(1, OID));
}

const entry: StashEntry = {
  index: 0,
  oid: STASH,
  message: "GitCRAFT: antes del cherry-pick de a1b2c3d",
  branch: "main",
  time: 0,
  untrackedOid: null,
} as unknown as StashEntry;

beforeEach(async () => {
  await i18n.changeLanguage("es");
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
});

describe("cherry-pick", () => {
  it("is offered in the detail panel of a commit not in the branch", async () => {
    const user = userEvent.setup();
    ipc.commitActions.mockResolvedValue({
      status: "ok",
      data: {
        oid: OID,
        branch: "main",
        inProgress: null,
        isMerge: false,
        inBranch: false,
        isHead: false,
      },
    });
    ipc.cherryPickPreview.mockResolvedValue({ status: "ok", data: preview() });
    render(
      <>
        <CommitActionButtons repoId={1} oid={OID} />
        <HistoryDialogs />
      </>,
    );
    await user.click(
      await screen.findByRole("button", { name: "Cherry-pick…" }),
    );
    expect(
      await screen.findByRole("alertdialog", {
        name: "¿Hacer cherry-pick de a1b2c3d en «main»?",
      }),
    ).toBeInTheDocument();
  });

  it("copies the commit and says so", async () => {
    const user = userEvent.setup();
    ipc.cherryPick.mockResolvedValue({
      status: "ok",
      data: { type: "done", head: "9".repeat(40), saved: null },
    });
    await show(preview());
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      "Se copia «topic edits f» a «main» como un commit nuevo.",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer cherry-pick" }),
    );
    expect(ipc.cherryPick).toHaveBeenCalledWith(1, OID, false, false);
    expect(
      await screen.findByText(
        "Cherry-pick hecho: «topic edits f» copiado en «main».",
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeNull();
  });

  it("lists what is in the way and saves it first", async () => {
    const user = userEvent.setup();
    await show(
      preview({
        blocked: "stagedChanges",
        blocking: [
          { path: "other.txt", untracked: false },
          { path: "new.txt", untracked: true },
        ],
      }),
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent("aunque sean de otros archivos");
    expect(dialog).toHaveTextContent("new.txt (sin seguimiento)");
    expect(dialog).toHaveTextContent(
      "«GitCRAFT: antes del cherry-pick de a1b2c3d»",
    );
    ipc.cherryPick.mockResolvedValue({
      status: "ok",
      data: { type: "done", head: "9".repeat(40), saved: STASH },
    });
    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y hacer cherry-pick",
      }),
    );
    expect(ipc.cherryPick).toHaveBeenCalledWith(1, OID, true, false);
    expect(
      await screen.findByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeInTheDocument();
  });

  it("recovers the saved changes with the stash's apply, keeping the entry", async () => {
    const user = userEvent.setup();
    ipc.listStashes.mockResolvedValue({ status: "ok", data: [entry] });
    ipc.applyStash.mockResolvedValue({
      status: "ok",
      data: { type: "applied", dropped: false },
    });
    useHistoryStore.setState({
      saved: { 1: { oid: STASH, short: "a1b2c3d" } },
    });
    render(<HistoryToast repoId={1} />);
    await user.click(
      screen.getByRole("button", { name: "Recuperar mis cambios" }),
    );
    // Not pop: the entry stays in the stash.
    expect(ipc.applyStash).toHaveBeenCalledWith(1, STASH, false, false, false);
    expect(useHistoryStore.getState().saved[1]).toBeUndefined();
  });

  it("offers them back only once the operation is over", () => {
    useHistoryStore.setState({
      saved: { 1: { oid: STASH, short: "a1b2c3d" } },
    });
    useWorkdirStore.setState({
      statuses: {
        1: {
          status: { ...cleanStatus(), inProgress: "cherryPick" },
          error: null,
        },
      },
    });
    render(<HistoryToast repoId={1} />);
    expect(
      screen.queryByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeNull();
    act(() =>
      useWorkdirStore.setState({
        statuses: { 1: { status: cleanStatus(), error: null } },
      }),
    );
    expect(
      screen.getByRole("button", { name: "Recuperar mis cambios" }),
    ).toBeInTheDocument();
  });

  it("announces conflicts, and with saved changes says aborting keeps them", async () => {
    const user = userEvent.setup();
    await show(
      preview({
        blocked: "overwrite",
        blocking: [{ path: "f.txt", untracked: false }],
        conflicts: ["f.txt"],
      }),
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(
      "El cherry-pick producirá conflictos en 1 archivo:",
    );
    expect(dialog).toHaveTextContent("no vuelven solos al abortar");
    ipc.cherryPick.mockResolvedValue({
      status: "ok",
      data: { type: "conflicted", files: ["f.txt"], saved: STASH },
    });
    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y hacer cherry-pick con conflictos",
      }),
    );
    expect(ipc.cherryPick).toHaveBeenCalledWith(1, OID, true, true);
    expect(useHistoryStore.getState().saved[1]?.oid).toBe(STASH);

    // The abort confirmation says it too.
    useRemoteStore.setState({
      dialog: { kind: "abort", repoId: 1, operation: "cherryPick" },
    });
    render(<AbortDialog />);
    expect(
      screen.getByRole("alertdialog", { name: "¿Abortar el cherry-pick?" }),
    ).toHaveTextContent("no vuelven solos al abortar");
  });

  it("says when conflicts are only an estimate", async () => {
    await show(preview({ conflicts: ["f.txt"], exact: false }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      "El cherry-pick podría producir conflictos en 1 archivo:",
    );
  });
});
