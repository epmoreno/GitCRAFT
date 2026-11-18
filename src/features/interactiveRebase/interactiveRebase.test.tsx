import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { setPlatformForTests } from "../../app/platform";
import i18n from "../../i18n";
import type { InteractivePreview, PlanPreview, RebasePlan } from "../../ipc";
import {
  ipc,
  oid,
  sampleLines,
  sampleRefs,
  streamEvents,
} from "../../test/ipc";
import { samplePrediction, samplePreview } from "../../test/interactiveRebase";
import { GraphView } from "../graph/GraphView";
import { HistoryToast } from "../history/HistoryDialogs";
import { InteractiveRebaseView, ROW_HEIGHT } from "./InteractiveRebaseView";
import { useInteractiveRebaseStore } from "./store";

let predict: (plan: RebasePlan) => Partial<PlanPreview> = () => ({});

async function open(
  preview: InteractivePreview = samplePreview(),
  rendered = false,
) {
  ipc.interactiveRebasePreview.mockResolvedValue({
    status: "ok",
    data: preview,
  });
  ipc.simulateRebasePlan.mockImplementation((_id, plan) =>
    Promise.resolve({
      status: "ok",
      data: samplePrediction(preview, plan, predict(plan)),
    }),
  );
  if (!rendered) {
    render(
      <>
        <InteractiveRebaseView repoId={1} />
        <HistoryToast repoId={1} />
      </>,
    );
  }
  await act(() => useInteractiveRebaseStore.getState().open(1, oid("a")));
  return screen.getByRole("region", {
    name: /Rebase interactivo de «main»|Interactive rebase of “main”/,
  });
}

/** The rows, top to bottom: their short ids. */
const order = (view: HTMLElement) =>
  within(view)
    .getAllByRole("listitem")
    .map((row) => row.querySelector("code")?.textContent);

const row = (view: HTMLElement, label: string) =>
  within(view)
    .getAllByRole("listitem")
    .find((r) => r.querySelector("code")?.textContent === label.repeat(7))!;

const actionOf = (view: HTMLElement, label: string) =>
  within(view).getByRole("combobox", {
    name: new RegExp(label.repeat(7)),
  }) as HTMLSelectElement;

/** The plan of the last prediction asked for. */
const lastPlan = () => ipc.simulateRebasePlan.mock.lastCall![1];

const apply = (view: HTMLElement) =>
  within(view).getByRole("button", { name: /^(Aplicar…|Apply…)$/ });

beforeEach(async () => {
  await i18n.changeLanguage("es");
  setPlatformForTests("mac");
  predict = () => ({});
  ipc.undoAvailable.mockResolvedValue({ status: "ok", data: true });
});

describe("the interactive rebase view", () => {
  it("lists the newest at the top and says how git applies them", async () => {
    const view = await open();
    expect(order(view)).toEqual(["ddddddd", "ccccccc", "bbbbbbb", "aaaaaaa"]);
    expect(view).toHaveTextContent(
      "↑ Más nuevo arriba · git aplica de abajo arriba (1 → 4)",
    );
    expect(view).toHaveTextContent(
      "Arrastra ⠿ o usa ⌥↑ / ⌥↓ para reordenar · 1–4 cambian la acción",
    );
    // The step each one is applied in: 1 at the bottom.
    expect(
      within(row(view, "a")).getByTitle("Se aplica en el paso 1"),
    ).toBeInTheDocument();
    // The chosen commit is selected and has the focus.
    expect(row(view, "a")).toHaveAttribute("aria-current", "true");
    expect(row(view, "a")).toHaveFocus();
    // Nothing to do yet.
    expect(
      await within(view).findByText(
        "El plan deja la rama como está: elige qué cambiar.",
      ),
    ).toBeInTheDocument();
    expect(apply(view)).toBeDisabled();
    expect(
      within(view).getByRole("button", { name: "Volver al plan original" }),
    ).toBeDisabled();
  });

  it("drops, rewords and combines, and predicts again", async () => {
    const user = userEvent.setup();
    const view = await open();
    await within(view).findByText(/El plan deja la rama/);

    await user.selectOptions(actionOf(view, "b"), "Descartar");
    expect(row(view, "b").className).toMatch(/dropped/);
    expect(
      within(view).getByRole("status", { name: "Previsión" }),
    ).toHaveTextContent("Calculando…");
    expect(
      await within(view).findByText(
        "Quedarán 3 commits desde el elegido · 1 descartado",
      ),
    ).toBeInTheDocument();

    // Edit message: starts with the commit's message.
    await user.selectOptions(actionOf(view, "c"), "Editar mensaje");
    await user.click(row(view, "c"));
    const editor = within(view).getByRole("textbox", {
      name: "Mensaje de ccccccc (Editar mensaje)",
    });
    expect(editor).toHaveValue("wip\n\nCuerpo de c");
    await user.clear(editor);
    await user.type(editor, "Añade la búsqueda");
    await within(view).findByText(/Quedarán/);
    expect(lastPlan().steps[2]).toEqual({
      oid: oid("c"),
      action: "reword",
      message: "Añade la búsqueda",
    });

    // Combine D with C below: C receives it, both messages to edit.
    await user.selectOptions(actionOf(view, "d"), "Combinar con el de abajo");
    expect(
      within(row(view, "c")).getByText("recibe 1 commit"),
    ).toBeInTheDocument();
    expect(
      within(row(view, "d")).getByTitle("Se combina con ccccccc, debajo"),
    ).toHaveTextContent("↓");
    await user.click(row(view, "d"));
    const combined = within(view).getByRole("textbox", {
      name: "Mensaje del commit combinado (2 commits: ccccccc, ddddddd)",
    });
    expect(combined).toHaveValue(
      "Añade la búsqueda\n\nArregla el tooltip\n\nCuerpo de d",
    );
    await within(view).findByText(/Quedarán 2 commits/);
    expect(lastPlan().steps[3]).toEqual({
      oid: oid("d"),
      action: "squash",
      message: "Añade la búsqueda\n\nArregla el tooltip\n\nCuerpo de d",
    });

    // Back to the original plan.
    await user.click(
      within(view).getByRole("button", { name: "Volver al plan original" }),
    );
    expect(actionOf(view, "b")).toHaveValue("pick");
    await within(view).findByText(/El plan deja la rama/);
  });

  it("reorders with the keyboard and changes the action with 1–4", async () => {
    const user = userEvent.setup();
    const view = await open();
    // ↑ selects the commit above; ⌥↑ moves it up (applied later).
    await user.keyboard("{ArrowUp}");
    expect(row(view, "b")).toHaveFocus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(order(view)).toEqual(["ddddddd", "bbbbbbb", "ccccccc", "aaaaaaa"]);
    expect(row(view, "b")).toHaveFocus();
    await user.keyboard("{Alt>}{ArrowDown}{ArrowDown}{/Alt}");
    expect(order(view)).toEqual(["ddddddd", "ccccccc", "aaaaaaa", "bbbbbbb"]);
    await user.keyboard("4");
    expect(actionOf(view, "b")).toHaveValue("drop");
    await user.keyboard("2");
    expect(actionOf(view, "b")).toHaveValue("reword");
    await user.keyboard("1");
    expect(actionOf(view, "b")).toHaveValue("pick");
    await within(view).findByText(/Quedarán 4 commits/);
    expect(lastPlan().steps.map((s) => s.oid)).toEqual(
      ["b", "a", "c", "d"].map(oid),
    );
  });

  it("reorders by dragging a row", async () => {
    const view = await open();
    const grip = row(view, "d").querySelector<HTMLElement>("[data-grip]")!;
    fireEvent.pointerDown(grip, { button: 0, clientY: 5 });
    fireEvent.pointerMove(window, { clientY: 2 * ROW_HEIGHT + 5 });
    // Where it would land is marked.
    expect(row(view, "b").className).toMatch(/dropBelow/);
    fireEvent.pointerUp(window, { clientY: 2 * ROW_HEIGHT + 5 });
    expect(order(view)).toEqual(["ccccccc", "bbbbbbb", "ddddddd", "aaaaaaa"]);
    expect(row(view, "d")).toHaveAttribute("aria-current", "true");
    // A click on the grip moves nothing.
    fireEvent.pointerDown(grip, { button: 0, clientY: 75 });
    fireEvent.pointerUp(window, { clientY: 76 });
    expect(order(view)).toEqual(["ccccccc", "bbbbbbb", "ddddddd", "aaaaaaa"]);
  });

  it("marks where it would stop, what becomes empty and what is published", async () => {
    const user = userEvent.setup();
    const p = samplePreview();
    p.commits[0]!.published = true;
    predict = (plan) =>
      plan.steps[0]!.oid === oid("b")
        ? {
            stopsAt: {
              step: 2,
              of: 4,
              commit: p.commits[0]!.commit,
              files: ["src/guía.ts"],
            },
            empty: [p.commits[2]!.commit],
            publishedIn: ["origin/main"],
          }
        : {};
    const view = await open(p);
    expect(within(row(view, "a")).getByText(/publicado/)).toBeInTheDocument();
    await user.click(row(view, "b"));
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    const panel = within(view).getByRole("status", { name: "Previsión" });
    expect(
      await within(panel).findByText(
        "⚠ Se detendrá en el paso 2 de 4 (aaaaaaa) con conflictos en:",
      ),
    ).toBeInTheDocument();
    expect(panel).toHaveTextContent("src/guía.ts");
    expect(panel).toHaveTextContent(
      "Se reescriben commits publicados en origin/main: para subirlos hará falta un push forzado.",
    );
    expect(
      within(row(view, "a")).getByText(/se detiene aquí/),
    ).toBeInTheDocument();
    expect(
      within(row(view, "c")).getByText("quedará vacío: git lo omitirá"),
    ).toBeInTheDocument();

    // The confirmation says it too, and applies with conflicts.
    await user.click(apply(view));
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(/ya están publicados \(origin\/main\)/);
    expect(dialog).toHaveTextContent(
      "Se detendrá en el paso 2 de 4 (aaaaaaa «Añade la guía») con conflictos en:",
    );
    ipc.interactiveRebase.mockResolvedValue({
      status: "ok",
      data: {
        outcome: { kind: "conflicted", files: ["src/guía.ts"], saved: null },
        undoId: null,
      },
    });
    await user.click(
      within(dialog).getByRole("button", { name: "Aplicar con conflictos" }),
    );
    expect(ipc.interactiveRebase).toHaveBeenCalledWith(1, {
      plan: expect.objectContaining({ expected: "fp-1" }),
      saveChanges: false,
      allowConflicts: true,
    });
    // Task 9's banner and conflict view take over.
    expect(useInteractiveRebaseStore.getState().view).toBeNull();
  });

  it("lists what is lost when dropping, and cancelling does nothing", async () => {
    const user = userEvent.setup();
    const view = await open();
    await user.keyboard("{ArrowUp}4");
    await within(view).findByText(/1 descartado/);
    await user.click(apply(view));
    const dialog = screen.getByRole("alertdialog", {
      name: "¿Aplicar el rebase interactivo de «main»?",
    });
    expect(dialog).toHaveTextContent("Se descarta 1 commit:");
    expect(dialog).toHaveTextContent("bbbbbbb Prueba temporal");
    expect(dialog).toHaveTextContent(
      "Puedes recuperarlos con «Deshacer» justo después, o más tarde desde el historial de «main».",
    );
    await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    // Esc and "Cancel" go back to the graph without touching anything.
    await user.click(row(view, "c"));
    await user.keyboard("{Escape}");
    expect(useInteractiveRebaseStore.getState().view).toBeNull();
    expect(ipc.interactiveRebase).not.toHaveBeenCalled();
    const again = await open(samplePreview(), true);
    await user.click(within(again).getByRole("button", { name: "Cancelar" }));
    expect(useInteractiveRebaseStore.getState().view).toBeNull();
    expect(ipc.interactiveRebase).not.toHaveBeenCalled();
  });

  it("Esc in the message goes back to the list first", async () => {
    const user = userEvent.setup();
    const view = await open();
    await user.keyboard("2");
    await user.click(
      within(view).getByRole("textbox", { name: /Mensaje de aaaaaaa/ }),
    );
    await user.keyboard("{Escape}");
    expect(useInteractiveRebaseStore.getState().view).not.toBeNull();
    expect(row(view, "a")).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(useInteractiveRebaseStore.getState().view).toBeNull();
  });

  it("applies and offers «Deshacer»", async () => {
    const user = userEvent.setup();
    const view = await open();
    await user.keyboard("{ArrowUp}{Alt>}{ArrowDown}{/Alt}");
    await within(view).findByText(/Quedarán 4 commits/);
    ipc.interactiveRebase.mockResolvedValue({
      status: "ok",
      data: {
        outcome: { kind: "done", head: oid("f"), saved: null },
        undoId: 7,
      },
    });
    await user.click(apply(view));
    await user.click(
      within(screen.getByRole("alertdialog")).getByRole("button", {
        name: "Aplicar",
      }),
    );
    expect(useInteractiveRebaseStore.getState().view).toBeNull();
    const toast = screen.getByRole("status");
    expect(toast).toHaveTextContent(
      "Rebase interactivo de «main» hecho: 4 commits reescritos o descartados.",
    );
    ipc.undoOperation.mockResolvedValue({ status: "ok", data: null });
    await user.click(screen.getByRole("button", { name: "Deshacer" }));
    expect(ipc.undoOperation).toHaveBeenCalledWith(1, 7);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Rebase interactivo deshecho: «main» está exactamente como antes.",
    );
  });

  it("explains refusals and what an old git cannot do", async () => {
    const refused = await open(
      samplePreview({ refused: "mergesInRange", commits: [] }),
    );
    expect(refused).toHaveTextContent(
      "Entre este commit y el último de «main» hay merges.",
    );
    expect(within(refused).queryByRole("listbox")).toBeNull();
    act(() => useInteractiveRebaseStore.getState().close());

    const old = await open(
      samplePreview({ messages: false, exact: false, gitVersion: "2.31.1" }),
      true,
    );
    expect(old).toHaveTextContent(
      "«Editar mensaje» y «Combinar» necesitan git 2.32 o posterior (tienes 2.31.1).",
    );
    const options = within(actionOf(old, "a")).getAllByRole("option");
    expect(options.map((o) => (o as HTMLOptionElement).disabled)).toEqual([
      false,
      true,
      true,
      false,
    ]);
    // Their keys do nothing either.
    fireEvent.keyDown(row(old, "a"), { key: "3" });
    expect(actionOf(old, "a")).toHaveValue("pick");
  });

  it("speaks English too", async () => {
    await i18n.changeLanguage("en");
    const user = userEvent.setup();
    const view = await open();
    expect(view).toHaveTextContent(
      "↑ Newest at the top · git applies them from the bottom up (1 → 4)",
    );
    await user.keyboard("{ArrowUp}3");
    expect(
      within(row(view, "a")).getByText("receives 1 commit"),
    ).toBeInTheDocument();
    expect(
      within(view).getByRole("textbox", {
        name: "Message of the combined commit (2 commits: aaaaaaa, bbbbbbb)",
      }),
    ).toBeInTheDocument();
    expect(
      await within(view).findByText(
        "3 commits will be left from the chosen one",
      ),
    ).toBeInTheDocument();
    expect(apply(view)).toBeEnabled();
  });
});

describe("opening it", () => {
  const info = {
    path: "/src/sample",
    name: "sample",
    gitDir: "/src/sample/.git",
    commonDir: "/src/sample/.git",
  };

  it("from a commit's menu, over the history", async () => {
    const user = userEvent.setup();
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
    const preview = samplePreview();
    ipc.interactiveRebasePreview.mockResolvedValue({
      status: "ok",
      data: preview,
    });
    ipc.simulateRebasePlan.mockImplementation((_id, plan) =>
      Promise.resolve({ status: "ok", data: samplePrediction(preview, plan) }),
    );
    render(<GraphView repoId={1} info={info} />);
    const list = await screen.findByRole("listbox", {
      name: "Historial de sample",
    });
    const commit = within(list).getAllByRole("option")[2]!;
    fireEvent.contextMenu(commit, { clientX: 40, clientY: 50 });
    await user.click(
      await screen.findByRole("menuitem", {
        name: "Rebase interactivo desde aquí…",
      }),
    );
    expect(ipc.interactiveRebasePreview).toHaveBeenCalledWith(
      1,
      sampleLines()[2]!.oid,
    );
    expect(
      await screen.findByRole("region", {
        name: "Rebase interactivo de «main»",
      }),
    ).toBeInTheDocument();
    // The search waits until the view is closed.
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("region", { name: "Rebase interactivo de «main»" }),
    ).toBeNull();
  });

  it("not for a commit outside the current branch", async () => {
    ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
    streamEvents([
      { type: "page", lines: sampleLines() },
      { type: "done", total: 6, suggestOptimize: false },
    ]);
    ipc.commitActions.mockImplementation((_id, commit) =>
      Promise.resolve({
        status: "ok",
        data: {
          oid: commit,
          branch: "main",
          inProgress: null,
          isMerge: false,
          inBranch: false,
          isHead: false,
        },
      }),
    );
    render(<GraphView repoId={1} info={info} />);
    const list = await screen.findByRole("listbox", {
      name: "Historial de sample",
    });
    fireEvent.contextMenu(within(list).getAllByRole("option")[0]!, {
      clientX: 40,
      clientY: 50,
    });
    const item = await screen.findByRole("menuitem", {
      name: "Rebase interactivo desde aquí…",
    });
    await vi.waitFor(() => expect(item).toBeDisabled());
    expect(item).toHaveAttribute(
      "title",
      "No está en «main»: el rebase interactivo es de commits de la rama actual.",
    );
  });
});
