import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { WorkingStatus } from "../../ipc";
import {
  cleanStatus,
  commandError,
  ipc,
  oid,
  sampleLines,
  sampleRefs,
} from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { useWorkdirStore } from "./store";
import { WorkingDirPanel } from "./WorkingDirPanel";

function withStatus(status: WorkingStatus) {
  useWorkdirStore.setState({
    statuses: { 1: { status, error: null } },
    drafts: {},
  });
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: sampleLines(),
        maxColumn: 1,
        refs: sampleRefs(),
        error: null,
        selectedOid: "worktree",
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

const staged = (): WorkingStatus => ({
  ...cleanStatus(),
  staged: [
    { path: "a.txt", oldPath: null, status: "added" },
    { path: "b.txt", oldPath: null, status: "modified" },
  ],
});

function renderBox() {
  render(<WorkingDirPanel repoId={1} />);
  const form = screen.getByRole("form", { name: "Crear commit" });
  return {
    form,
    summary: within(form).getByRole("textbox", { name: "Resumen" }),
    description: within(form).getByRole("textbox", { name: "Descripción" }),
    amend: within(form).getByRole("checkbox", {
      name: /Modificar el último commit/,
    }),
    button: () =>
      within(form).getByRole("button", { name: /Commit|Modificar/ }),
  };
}

describe("CommitBox", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("explains why committing is not possible yet", async () => {
    const user = userEvent.setup();
    withStatus({
      ...cleanStatus(),
      unstaged: [{ path: "a.txt", oldPath: null, status: "modified" }],
    });
    const box = renderBox();
    expect(box.button()).toBeDisabled();
    expect(
      within(box.form).getByText(
        "Prepara algún cambio para poder hacer commit.",
      ),
    ).toBeVisible();

    withStatus(staged());
    await user.type(box.summary, "   ");
    expect(
      within(box.form).getByText("Escribe un resumen para el commit."),
    ).toBeVisible();
  });

  it("blocks committing while there are conflicts", () => {
    withStatus({
      ...staged(),
      conflicted: [{ path: "c.txt", kind: "bothModified" }],
    });
    const box = renderBox();
    expect(box.button()).toBeDisabled();
    expect(within(box.form).getByText(/Resuelve los conflictos/)).toBeVisible();
  });

  it("commits summary and description, then selects the new commit", async () => {
    const user = userEvent.setup();
    withStatus(staged());
    const box = renderBox();
    expect(box.button()).toHaveTextContent("Commit de 2 archivos");

    await user.type(box.summary, "Añade a y b");
    await user.type(box.description, "Porque sí.");
    await user.click(box.button());

    expect(ipc.createCommit).toHaveBeenCalledWith(
      1,
      "Añade a y b",
      "Porque sí.",
      false,
    );
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("9"));
    expect(ipc.workingStatus).toHaveBeenCalled();
    expect(useWorkdirStore.getState().drafts[1]).toEqual({
      summary: "",
      description: "",
      amend: false,
    });
  });

  it("commits with Cmd+Enter", async () => {
    const user = userEvent.setup();
    withStatus(staged());
    const box = renderBox();
    await user.type(box.summary, "Quick{Meta>}{Enter}{/Meta}");
    expect(ipc.createCommit).toHaveBeenCalledWith(1, "Quick", "", false);
  });

  it("counts summary characters and warns past 72", async () => {
    const user = userEvent.setup();
    withStatus(staged());
    const box = renderBox();
    await user.type(box.summary, "ñandú");
    expect(within(box.form).getByText("5/72")).toBeVisible();
    await user.clear(box.summary);
    await user.type(box.summary, "x".repeat(73));
    const counter = within(box.form).getByText("73/72");
    expect(counter).toHaveAttribute(
      "title",
      expect.stringMatching(/72 caracteres/),
    );
  });

  it("keeps the draft when the panel is closed and reopened", async () => {
    const user = userEvent.setup();
    withStatus(staged());
    const { unmount } = render(<WorkingDirPanel repoId={1} />);
    await user.type(
      screen.getByRole("textbox", { name: "Resumen" }),
      "Borrador",
    );
    unmount();
    render(<WorkingDirPanel repoId={1} />);
    expect(screen.getByRole("textbox", { name: "Resumen" })).toHaveValue(
      "Borrador",
    );
  });

  it("shows why a commit failed, with git's output", async () => {
    const user = userEvent.setup();
    ipc.createCommit.mockResolvedValue({
      status: "error",
      error: commandError(
        "unknown",
        "$ git commit\nlint failed: trailing spaces",
      ),
    });
    withStatus(staged());
    const box = renderBox();
    await user.type(box.summary, "Intento");
    await user.click(box.button());
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/lint failed/)).toHaveClass("selectable");
    // The message is kept so the user can fix the problem and retry.
    expect(box.summary).toHaveValue("Intento");
  });

  describe("amend", () => {
    const lastCommit = (publishedIn: string[]) => ({
      status: "ok" as const,
      data: { oid: oid("e"), summary: "E", body: "Cuerpo de E", publishedIn },
    });

    it("prefills the last message and allows amending without staged changes", async () => {
      const user = userEvent.setup();
      ipc.amendInfo.mockResolvedValue(lastCommit([]));
      withStatus(cleanStatus());
      const box = renderBox();
      await user.click(box.amend);
      expect(box.summary).toHaveValue("E");
      expect(box.description).toHaveValue("Cuerpo de E");
      expect(box.button()).toHaveTextContent("Modificar el último commit");

      await user.type(box.summary, " (corregido)");
      await user.click(box.button());
      expect(ipc.createCommit).toHaveBeenCalledWith(
        1,
        "E (corregido)",
        "Cuerpo de E",
        true,
      );
    });

    it("does not overwrite a message the user already wrote", async () => {
      const user = userEvent.setup();
      ipc.amendInfo.mockResolvedValue(lastCommit([]));
      withStatus(staged());
      const box = renderBox();
      await user.type(box.summary, "Mi mensaje");
      await user.click(box.amend);
      expect(box.summary).toHaveValue("Mi mensaje");
    });

    it("asks before amending a published commit, explaining the consequences", async () => {
      const user = userEvent.setup();
      ipc.amendInfo.mockResolvedValue(lastCommit(["origin/main"]));
      withStatus(staged());
      const box = renderBox();
      await user.click(box.amend);
      await user.click(box.button());

      const dialog = screen.getByRole("alertdialog", {
        name: "¿Modificar un commit ya publicado?",
      });
      expect(dialog).toHaveTextContent(
        /«E» \(eeeeeee\) ya está en origin\/main/,
      );
      expect(dialog).toHaveTextContent(/push forzado/);
      expect(dialog).toHaveTextContent(/reflog/);
      // Focus starts on Cancel: Enter by reflex does not rewrite history.
      expect(
        within(dialog).getByRole("button", { name: "Cancelar" }),
      ).toHaveFocus();

      await user.keyboard("{Escape}");
      expect(screen.queryByRole("alertdialog")).toBeNull();
      expect(ipc.createCommit).not.toHaveBeenCalled();

      await user.click(box.button());
      await user.click(
        screen.getByRole("button", { name: "Modificar de todos modos" }),
      );
      expect(ipc.createCommit).toHaveBeenCalledWith(
        1,
        "E",
        "Cuerpo de E",
        true,
      );
    });

    it("cannot be enabled before the first commit", async () => {
      const user = userEvent.setup();
      ipc.amendInfo.mockResolvedValue({ status: "ok", data: null });
      withStatus(staged());
      const box = renderBox();
      await user.click(box.amend);
      expect(box.amend).not.toBeChecked();
      expect(
        screen.getByText("Todavía no hay ningún commit que corregir."),
      ).toBeVisible();
    });

    it("explains why amend could not be turned on", async () => {
      const user = userEvent.setup();
      ipc.amendInfo.mockResolvedValue({
        status: "error",
        error: commandError(
          "indexLocked",
          "fatal: Unable to create index.lock",
        ),
      });
      withStatus(staged());
      const box = renderBox();
      await user.click(box.amend);
      expect(box.amend).not.toBeChecked();
      expect(screen.getByRole("alert")).toHaveTextContent(
        /No se pudo preparar la corrección del último commit\. Otro proceso de Git/,
      );
    });
  });
});
