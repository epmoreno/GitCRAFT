import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { WorkingStatus } from "../../ipc";
import {
  cleanStatus,
  commandError,
  ipc,
  sampleLines,
  streamEvents,
} from "../../test/ipc";
import { DetailPanel } from "../detail/DetailPanel";
import { GraphView } from "../graph/GraphView";
import { useGraphStore, WORKTREE } from "../graph/store";
import { changeCounts, useWorkdirStore } from "./store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

export function busyStatus(): WorkingStatus {
  return {
    ...cleanStatus(),
    staged: [
      { path: "src/app.ts", oldPath: null, status: "modified" },
      { path: "docs/new name.md", oldPath: "docs/old.md", status: "renamed" },
    ],
    unstaged: [{ path: "README.md", oldPath: null, status: "modified" }],
    untracked: [{ path: "notes/", oldPath: null, status: "untracked" }],
    conflicted: [{ path: "conflict.txt", kind: "bothModified" }],
  };
}

describe("working directory store", () => {
  it("counts changes, untracked files as unstaged", () => {
    expect(changeCounts(busyStatus())).toEqual({
      staged: 2,
      unstaged: 2,
      conflicted: 1,
      total: 5,
    });
    expect(changeCounts(null).total).toBe(0);
  });

  it("keeps only the latest answer and the last good status on errors", async () => {
    let first:
      ((v: Awaited<ReturnType<typeof ipc.workingStatus>>) => void) | undefined;
    ipc.workingStatus
      .mockImplementationOnce(() => new Promise((resolve) => (first = resolve)))
      .mockResolvedValueOnce({ status: "ok", data: busyStatus() })
      .mockResolvedValueOnce({
        status: "error",
        error: commandError("unknown"),
      });

    const stale = useWorkdirStore.getState().refresh(1);
    await useWorkdirStore.getState().refresh(1);
    first?.({ status: "ok", data: cleanStatus() });
    await stale;
    expect(useWorkdirStore.getState().statuses[1]?.status).toEqual(
      busyStatus(),
    );

    await useWorkdirStore.getState().refresh(1);
    expect(useWorkdirStore.getState().statuses[1]?.status).toEqual(
      busyStatus(),
    );
    expect(useWorkdirStore.getState().statuses[1]?.error?.kind).toBe("unknown");
  });
});

describe("uncommitted changes", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
  });

  it("are not shown when the working directory is clean", async () => {
    useWorkdirStore.setState({
      statuses: { 1: { status: cleanStatus(), error: null } },
    });
    render(<GraphView repoId={1} info={info} />);
    await screen.findByText("6 commits");
    expect(
      screen.queryByRole("button", { name: /Cambios sin confirmar/ }),
    ).toBeNull();
  });

  it("appear at the top of the history and open the working directory panel", async () => {
    const user = userEvent.setup();
    useWorkdirStore.setState({
      statuses: { 1: { status: busyStatus(), error: null } },
    });
    render(
      <>
        <GraphView repoId={1} info={info} />
        <DetailPanel repoId={1} />
      </>,
    );
    const row = await screen.findByRole("button", {
      name: /Cambios sin confirmar/,
    });
    expect(row).toHaveTextContent("1 conflicto");
    expect(row).toHaveTextContent("2 preparados");
    expect(row).toHaveTextContent("2 sin preparar");

    await user.click(row);
    expect(row).toHaveAttribute("aria-pressed", "true");
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(WORKTREE);
    expect(ipc.commitDetails).not.toHaveBeenCalled();

    const panel = screen.getByRole("complementary", { name: "Detalle" });
    const conflicts = within(panel).getByRole("region", { name: "Conflictos" });
    expect(conflicts).toHaveTextContent("conflict.txtmodificado en ambos");
    const staged = within(panel).getByRole("region", { name: "Preparados" });
    expect(within(staged).getAllByRole("listitem")).toHaveLength(2);
    expect(staged).toHaveTextContent("antes: docs/old.md");
    const unstaged = within(panel).getByRole("region", {
      name: "Sin preparar",
    });
    expect(
      within(unstaged).getByLabelText("Sin seguimiento"),
    ).toHaveTextContent("?");
    expect(within(unstaged).getByLabelText("Modificado")).toBeVisible();
  });
});
