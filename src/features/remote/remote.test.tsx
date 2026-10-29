import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { Toolbar } from "../../app/Toolbar";
import type { RepoRefs } from "../../ipc";
import {
  cleanStatus,
  commandError,
  ipc,
  opened,
  sampleRefs,
} from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { useRepoStore } from "../repo/store";
import { Sidebar } from "../sidebar/Sidebar";
import { InProgressBanner, RemoteDialogs } from "./RemoteDialogs";
import { RemoteProgress, RemoteToast } from "./RemoteStatus";
import { useWorkdirStore } from "../workdir/store";

function renderToolbar(refs: RepoRefs = sampleRefs()) {
  useRepoStore.setState({ tabs: [opened(1, "/src/sample")], activeId: 1 });
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs,
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
  ipc.getRefs.mockResolvedValue({ status: "ok", data: refs });
  render(
    <>
      <Toolbar />
      <Sidebar repoId={1} />
      <InProgressBanner repoId={1} />
      <RemoteProgress />
      <RemoteToast />
      <RemoteDialogs />
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

describe("ahead/behind", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("shows commits to push and to pull on the branch and in the sidebar", () => {
    const refs = sampleRefs();
    const main = refs.refs[0]!;
    main.ahead = 2;
    main.behind = 1;
    refs.refs[1] = {
      ...refs.refs[1]!,
      upstream: "origin/feature",
      upstreamGone: true,
    };
    renderToolbar(refs);

    const current = screen.getByLabelText(/^main\. /);
    expect(current).toHaveTextContent("main↑2 ↓1");
    expect(current).toHaveAccessibleName(
      "main. Sigue a «origin/main» · 2 commits por subir (push) · 1 commit por bajar (pull)",
    );

    const local = screen
      .getByRole("button", { name: /Ramas locales/ })
      .closest("section") as HTMLElement;
    expect(
      within(local).getByRole("button", { name: /^main/ }),
    ).toHaveTextContent("origin/main ↑2 ↓1");
    const feature = within(local).getByRole("button", { name: /^feature/ });
    expect(feature).toHaveTextContent("origin/feature (borrada en el remoto)");
    expect(feature.title).toContain("ya no existe en el remoto");
  });

  it("says when the branch is not published", () => {
    const refs = sampleRefs();
    refs.head = { type: "branch", name: "feature", oid: refs.refs[1]!.target };
    renderToolbar(refs);
    expect(screen.getByLabelText(/^feature\. /)).toHaveTextContent(
      "featuresin upstream",
    );
  });
});

describe("pull", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  const options = (allowConflicts: boolean, saveLocal: boolean) =>
    [1, { allowConflicts, saveLocal }, expect.any(Function)] as const;

  it("pulls and says how", async () => {
    const user = userEvent.setup();
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Pull" }));
    expect(ipc.pullBranch).toHaveBeenCalledWith(...options(false, false));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Pull completado: 1 commit de «origin/main» (avance rápido, sin commit de merge).",
    );
  });

  it("never overwrites uncommitted changes: explains and offers to stash them", async () => {
    const user = userEvent.setup();
    ipc.pullBranch.mockResolvedValueOnce({
      status: "ok",
      data: {
        type: "blocked",
        files: [{ path: "lista.txt", untracked: false }],
        reason: "overwrite",
      },
    });
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Pull" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "No se puede hacer pull todavía",
    });
    expect(within(dialog).getByText("lista.txt")).toBeVisible();
    expect(within(dialog).getByText("No se ha tocado nada.")).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: "Cancelar" }),
    ).toHaveFocus();

    await user.click(
      within(dialog).getByRole("button", {
        name: "Guardar mis cambios y hacer pull",
      }),
    );
    expect(ipc.pullBranch).toHaveBeenLastCalledWith(...options(false, true));
    expect(await screen.findByRole("status")).toHaveTextContent(
      /GitCRAFT: antes de hacer pull/,
    );
  });

  it("reports conflicts before pulling and only goes on if asked", async () => {
    const user = userEvent.setup();
    ipc.pullBranch.mockResolvedValueOnce({
      status: "ok",
      data: {
        type: "wouldConflict",
        upstream: "origin/main",
        files: ["lista.txt"],
        exact: true,
      },
    });
    ipc.pullBranch.mockResolvedValueOnce({
      status: "ok",
      data: { type: "conflicted", files: ["lista.txt"], mode: "merge" },
    });
    renderToolbar();
    await user.click(screen.getByRole("button", { name: "Pull" }));
    const dialog = await screen.findByRole("alertdialog", {
      name: "El pull produciría conflictos",
    });
    expect(
      within(dialog).getByText(/1 archivo ha cambiado tanto en tu rama/),
    ).toBeVisible();
    expect(within(dialog).getByText(/Podrás abortarlo/)).toBeVisible();
    await user.click(
      within(dialog).getByRole("button", { name: "Hacer pull con conflictos" }),
    );
    expect(ipc.pullBranch).toHaveBeenLastCalledWith(...options(true, false));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "El merge se ha detenido con conflictos en 1 archivo:",
    );
  });

  it("chooses merge or rebase per repository", async () => {
    const user = userEvent.setup();
    renderToolbar();
    await user.click(
      screen.getByRole("button", { name: "Estrategia de pull" }),
    );
    expect(
      screen.getByRole("menuitem", {
        name: /configuración de Git \(ahora: merge\)/,
      }),
    ).toHaveTextContent("✓");
    ipc.getPullMode.mockResolvedValue({
      status: "ok",
      data: { mode: "rebase", configured: "merge", chosen: true },
    });
    await user.click(
      screen.getByRole("menuitem", { name: /Rebase para este repositorio/ }),
    );
    expect(ipc.setPullMode).toHaveBeenCalledWith(1, "rebase");
    expect(
      await screen.findByRole("button", { name: /^Pull\s*\(rebase\)$/ }),
    ).toBeVisible();
  });

  it("offers to abort a merge left waiting on conflicts", async () => {
    const user = userEvent.setup();
    useWorkdirStore.setState({
      statuses: {
        1: {
          status: {
            ...cleanStatus(),
            inProgress: "merge",
            conflicted: [{ path: "lista.txt", kind: "bothModified" }],
          },
          error: null,
        },
      },
    });
    renderToolbar();
    const banner = screen.getByText(/Hay un merge a medias/);
    expect(banner).toHaveTextContent("Queda 1 archivo con conflictos");
    await user.click(screen.getByRole("button", { name: "Abortar…" }));
    const dialog = screen.getByRole("alertdialog", {
      name: "¿Abortar el merge?",
    });
    expect(ipc.abortInProgress).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Abortar" }));
    expect(ipc.abortInProgress).toHaveBeenCalledWith(1);
    expect(await screen.findByText(/Merge abortado/)).toBeVisible();
  });
});
