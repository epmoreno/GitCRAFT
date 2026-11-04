import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { PublishInfo } from "../../ipc";
import { commandError, emptyRefs, ipc, oid } from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { useSettingsStore } from "../settings/store";
import { formatSize, PublishDialog } from "./PublishDialog";
import { PublishButton } from "./PublishButton";
import { usePublishStore } from "./store";

const info = (overrides: Partial<PublishInfo> = {}): PublishInfo => ({
  preview: {
    branch: "main",
    commits: 12,
    sizeBytes: 3_400_000,
    hasGitignore: false,
    largeFiles: [{ path: "assets/video.mov", size: 72_000_000 }],
    buildDirs: ["node_modules"],
  },
  suggestedName: "Mi-Proyecto",
  account: { login: "ana", active: true, valid: true, gitProtocol: "https" },
  ghInstalled: true,
  ...overrides,
});

const published = {
  fullName: "ana/Mi-Proyecto",
  webUrl: "https://github.com/ana/Mi-Proyecto",
  remoteUrl: "https://github.com/ana/Mi-Proyecto.git",
};

function withRefs(
  remotes: string[],
  head: ReturnType<typeof emptyRefs>["head"] = {
    type: "branch",
    name: "main",
    oid: oid("a"),
  },
) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: { ...emptyRefs(), head, remotes },
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

async function openDialog(data: PublishInfo = info()) {
  ipc.publishInfo.mockResolvedValue({ status: "ok", data });
  render(<PublishDialog />);
  await act(() => usePublishStore.getState().open(1));
  return screen.getByRole("dialog", { name: "Publicar en GitHub" });
}

describe("Publish on GitHub: the button", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("only appears in a repository without remotes", () => {
    withRefs(["origin"]);
    const { unmount } = render(<PublishButton repoId={1} />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    unmount();
    withRefs([]);
    render(<PublishButton repoId={1} />);
    expect(
      screen.getByRole("button", { name: "Publicar en GitHub" }),
    ).toBeEnabled();
  });

  it("is disabled, saying why, without commits or on a detached HEAD", () => {
    withRefs([], { type: "branch", name: "main", oid: null });
    const { unmount } = render(<PublishButton repoId={1} />);
    const button = screen.getByRole("button", { name: "Publicar en GitHub" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(
      "Haz al menos un commit para poder publicarlo",
    );
    unmount();
    withRefs([], { type: "detached", oid: oid("a") });
    render(<PublishButton repoId={1} />);
    expect(screen.getByRole("button")).toHaveAccessibleDescription(
      /HEAD separado/,
    );
  });
});

describe("Publish on GitHub: the dialog", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("proposes the folder's name, private by default, in the personal account", async () => {
    const dialog = await openDialog();
    expect(ipc.publishInfo).toHaveBeenCalledWith(1);
    expect(
      within(dialog).getByRole("textbox", { name: "Nombre del repositorio" }),
    ).toHaveValue("Mi-Proyecto");
    expect(dialog).toHaveTextContent("@ana /");
    expect(
      within(dialog).getByRole("radio", { name: /Privado/ }),
    ).toBeChecked();
    expect(
      within(dialog).getByRole("button", { name: "Publicar como privado" }),
    ).toBeEnabled();
  });

  it("summarizes what will be uploaded and warns without blocking", async () => {
    const dialog = await openDialog();
    const summary = within(dialog).getByRole("region", {
      name: "Qué se va a subir",
    });
    expect(summary).toHaveTextContent("La rama main: 12 commits, unos 3,4 MB.");
    const warnings = within(summary).getAllByRole("listitem");
    expect(warnings).toHaveLength(3);
    expect(warnings[0]).toHaveTextContent("no tiene .gitignore");
    expect(warnings[1]).toHaveTextContent("assets/video.mov ocupa 72 MB");
    expect(warnings[2]).toHaveTextContent(
      "La carpeta node_modules está commiteada",
    );
    expect(summary).toHaveTextContent(
      "Son solo avisos: puedes publicar igualmente.",
    );
  });

  it("has no warnings when everything looks fine", async () => {
    const clean = info();
    clean.preview = {
      ...clean.preview,
      hasGitignore: true,
      largeFiles: [],
      buildDirs: [],
    };
    const dialog = await openDialog(clean);
    expect(
      within(dialog).queryByRole("list", { name: "Avisos" }),
    ).not.toBeInTheDocument();
  });

  it("refuses an invalid name", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog();
    const name = within(dialog).getByRole("textbox", {
      name: "Nombre del repositorio",
    });
    await user.clear(name);
    await user.type(name, "-mal");
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(
      within(dialog).getByRole("button", { name: "Publicar como privado" }),
    ).toBeDisabled();
  });

  it("creates the repository and then pushes the branch with its upstream, never forced", async () => {
    const user = userEvent.setup();
    ipc.publishRepository.mockResolvedValue({ status: "ok", data: published });
    ipc.pushBranch.mockResolvedValue({
      status: "ok",
      data: {
        type: "pushed",
        upstream: "origin/main",
        commits: 12,
        setUpstream: true,
        forced: false,
      },
    });
    const dialog = await openDialog();
    await user.type(
      within(dialog).getByRole("textbox", { name: "Descripción (opcional)" }),
      "  Mi app  ",
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Publicar como privado" }),
    );
    expect(ipc.publishRepository).toHaveBeenCalledWith(
      1,
      "Mi-Proyecto",
      "Mi app",
      true,
    );
    expect(ipc.pushBranch).toHaveBeenCalledWith(
      1,
      { publishTo: "origin", forceLease: null },
      expect.any(Function),
    );
    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "Publicado en https://github.com/ana/Mi-Proyecto",
    );
  });

  it("publishes as public only when chosen", async () => {
    const user = userEvent.setup();
    ipc.publishRepository.mockResolvedValue({
      status: "error",
      error: commandError("ghFailed"),
    });
    const dialog = await openDialog();
    await user.click(within(dialog).getByRole("radio", { name: /Público/ }));
    await user.click(
      within(dialog).getByRole("button", { name: "Publicar como público" }),
    );
    expect(ipc.publishRepository).toHaveBeenCalledWith(
      1,
      "Mi-Proyecto",
      null,
      false,
    );
  });

  it("explains an existing name, keeps the form and pushes nothing", async () => {
    const user = userEvent.setup();
    ipc.publishRepository.mockResolvedValue({
      status: "error",
      error: commandError("ghRepoExists"),
    });
    const dialog = await openDialog();
    await user.click(
      within(dialog).getByRole("button", { name: "Publicar como privado" }),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Ya tienes un repositorio con ese nombre en GitHub. No se ha creado ni cambiado nada",
    );
    expect(
      within(dialog).getByRole("textbox", { name: "Nombre del repositorio" }),
    ).toBeVisible();
    expect(ipc.pushBranch).not.toHaveBeenCalled();
  });

  it("says what is left when the push fails after creating the repository", async () => {
    const user = userEvent.setup();
    ipc.publishRepository.mockResolvedValue({ status: "ok", data: published });
    ipc.pushBranch.mockResolvedValue({
      status: "error",
      error: commandError("remoteUnreachable"),
    });
    const dialog = await openDialog();
    await user.click(
      within(dialog).getByRole("button", { name: "Publicar como privado" }),
    );
    const alert = await within(dialog).findByText(/se ha creado en GitHub/);
    expect(alert).toHaveTextContent(
      "El repositorio ana/Mi-Proyecto se ha creado en GitHub y se ha añadido origin (https://github.com/ana/Mi-Proyecto.git), pero el push no ha terminado. No se ha borrado nada: reinténtalo con Push.",
    );
  });

  it("without a GitHub session, takes you to Settings", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(info({ account: null }));
    expect(dialog).toHaveTextContent("No hay sesión de GitHub en gh.");
    expect(within(dialog).queryByRole("textbox")).not.toBeInTheDocument();
    await user.click(
      within(dialog).getByRole("button", { name: "Ir a Ajustes" }),
    );
    expect(
      screen.queryByRole("dialog", { name: "Publicar en GitHub" }),
    ).not.toBeInTheDocument();
    expect(useSettingsStore.getState()).toMatchObject({
      open: true,
      section: "github",
    });
  });

  it("formats sizes in the UI language", () => {
    expect(formatSize(512, "es")).toBe("512 B");
    expect(formatSize(3_400_000, "es")).toBe("3,4 MB");
    expect(formatSize(3_400_000, "en")).toBe("3.4 MB");
    expect(formatSize(150_000_000, "en")).toBe("150 MB");
  });
});
