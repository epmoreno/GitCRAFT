import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { commandError, ipc, sampleLines, streamEvents } from "../../test/ipc";
import { GraphView } from "./GraphView";
import { progressPercent } from "./OptimizeNotice";

const info = {
  path: "/src/big",
  name: "big",
  gitDir: "/src/big/.git",
  commonDir: "/src/big/.git",
};

function renderLargeRepo(suggestOptimize = true) {
  streamEvents([
    { type: "page", lines: sampleLines() },
    { type: "done", total: 6, suggestOptimize },
  ]);
  return render(<GraphView repoId={1} info={info} />);
}

describe("OptimizeNotice", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("is only shown when the backend suggests it", async () => {
    renderLargeRepo(false);
    await screen.findByText("6 commits");
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("explains what optimizing does and never runs git on its own", async () => {
    renderLargeRepo();
    const notice = await screen.findByRole("status");
    expect(notice).toHaveTextContent(/6 commits y no tiene el índice/);
    expect(notice).toHaveTextContent(/no cambia commits, ramas ni archivos/);
    expect(ipc.optimizeRepository).not.toHaveBeenCalled();
  });

  it("runs with progress and reports success", async () => {
    const user = userEvent.setup();
    let finish: (() => void) | undefined;
    ipc.optimizeRepository.mockImplementation(async (_id, onProgress) => {
      onProgress("Writing out commit graph in 4 passes:  50% (2/4)");
      await new Promise<void>((resolve) => (finish = resolve));
      return { status: "ok", data: null };
    });
    renderLargeRepo();
    const notice = await screen.findByRole("status");

    await user.click(
      within(notice).getByRole("button", { name: "Optimizar repositorio" }),
    );
    expect(ipc.optimizeRepository).toHaveBeenCalledWith(
      1,
      expect.any(Function),
    );
    const bar = await within(notice).findByRole("progressbar");
    expect(bar).toHaveAttribute("value", "50");

    await user.click(within(notice).getByRole("button", { name: "Cancelar" }));
    expect(ipc.cancelOperation).toHaveBeenCalledWith(1);

    finish?.();
    expect(
      await within(notice).findByText(/Repositorio optimizado/),
    ).toBeVisible();
    await user.click(within(notice).getByRole("button", { name: "Cerrar" }));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("returns to the offer when cancelled and shows errors with git output", async () => {
    const user = userEvent.setup();
    ipc.optimizeRepository
      .mockResolvedValueOnce({
        status: "error",
        error: commandError("cancelled"),
      })
      .mockResolvedValueOnce({
        status: "error",
        error: commandError(
          "unknown",
          "$ git commit-graph write\nfatal: disk full",
        ),
      });
    renderLargeRepo();
    const notice = await screen.findByRole("status");
    const run = () =>
      user.click(
        within(notice).getByRole("button", { name: "Optimizar repositorio" }),
      );

    await run();
    expect(
      await within(notice).findByRole("button", {
        name: "Optimizar repositorio",
      }),
    ).toBeVisible();

    await run();
    expect(
      await within(notice).findByText(/No se pudo optimizar/),
    ).toBeVisible();
    expect(within(notice).getByText(/fatal: disk full/)).toHaveClass(
      "selectable",
    );
  });

  it("can be dismissed for this repository", async () => {
    const user = userEvent.setup();
    renderLargeRepo();
    const notice = await screen.findByRole("status");
    await user.click(
      within(notice).getByRole("button", { name: "No mostrar más" }),
    );
    expect(ipc.dismissOptimizeNotice).toHaveBeenCalledWith(1);
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("progressPercent", () => {
  it("reads the last percentage of a git progress line", () => {
    expect(
      progressPercent("Writing out commit graph in 4 passes:  75% (3/4)"),
    ).toBe(75);
    expect(
      progressPercent("Expanding reachable commits in commit graph: 1234"),
    ).toBeNull();
    expect(progressPercent("a 10% b 100%, done.")).toBe(100);
  });
});
