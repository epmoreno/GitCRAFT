import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { Reflog, ReflogEntry } from "../../ipc";
import { cleanStatus, ipc, oid, sampleRefs } from "../../test/ipc";
import { useBranchStore } from "../branches/store";
import { useGraphStore } from "../graph/store";
import { Sidebar } from "../sidebar/Sidebar";
import { useWorkdirStore } from "../workdir/store";
import { ReflogDialog } from "./ReflogDialog";
import { useReflogStore } from "./store";

const now = Math.floor(Date.now() / 1000);

function entry(
  index: number,
  overrides: Partial<ReflogEntry> = {},
): ReflogEntry {
  const n = String(index + 1);
  return {
    index,
    oid: oid(n),
    short: n.repeat(7),
    time: now - (index + 1) * 600,
    message: `commit: Cambio ${n}`,
    subject: `Cambio ${n}`,
    onlyHere: false,
    ...overrides,
  };
}

function reflog(overrides: Partial<Reflog> = {}): Reflog {
  return {
    refname: "refs/heads/main",
    entries: [
      entry(0),
      entry(1, {
        message: "reset: moving to HEAD~1",
        subject: "Perdido",
        onlyHere: true,
      }),
      entry(2),
    ],
    more: false,
    expiry: {
      reachable: { type: "days", days: 90 },
      unreachable: { type: "days", days: 30 },
    },
    ...overrides,
  };
}

function withRefs() {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: sampleRefs(),
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

async function openFor(refname: string, data: Reflog = reflog({ refname })) {
  ipc.branchReflog.mockResolvedValue({ status: "ok", data });
  render(<ReflogDialog />);
  await act(() => useReflogStore.getState().open(1, refname));
  return screen.getByRole("dialog");
}

/** Opens the menu of the entry whose subject is `subject`. */
async function menuOf(subject: string) {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: new RegExp(subject) }));
  return screen.getByRole("menu");
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  withRefs();
  useWorkdirStore.setState({
    statuses: { 1: { status: cleanStatus(), error: null } },
  });
});

describe("where it opens", () => {
  it("a branch's menu has its history; the current one also HEAD's", async () => {
    ipc.branchReflog.mockResolvedValue({ status: "ok", data: reflog() });
    const user = userEvent.setup();
    render(
      <>
        <Sidebar repoId={1} />
        <ReflogDialog />
      </>,
    );
    fireEvent.contextMenu(screen.getByRole("button", { name: /^feature/ }));
    let menu = screen.getByRole("menu");
    expect(
      within(menu).queryByRole("menuitem", { name: "Historial de HEAD…" }),
    ).toBeNull();
    await user.click(
      within(menu).getByRole("menuitem", { name: "Historial de «feature»…" }),
    );
    expect(ipc.branchReflog).toHaveBeenCalledWith(1, "refs/heads/feature");
    await user.click(screen.getByRole("button", { name: "Cerrar" }));

    const [main] = screen.getAllByRole("button", { name: /^main/ });
    fireEvent.contextMenu(main as HTMLElement);
    menu = screen.getByRole("menu");
    await user.click(
      within(menu).getByRole("menuitem", { name: "Historial de HEAD…" }),
    );
    expect(ipc.branchReflog).toHaveBeenLastCalledWith(1, "HEAD");
    expect(
      await screen.findByRole("heading", { name: "Historial de HEAD" }),
    ).toBeVisible();
  });

  it("remote branches have none", () => {
    render(<Sidebar repoId={1} />);
    const [, remote] = screen.getAllByRole("button", { name: /^main/ });
    fireEvent.contextMenu(remote as HTMLElement);
    expect(
      within(screen.getByRole("menu")).queryByRole("menuitem", {
        name: /^Historial/,
      }),
    ).toBeNull();
  });
});

describe("the dialog", () => {
  it("lists the entries, newest first, as git noted them", async () => {
    const dialog = await openFor("refs/heads/main");
    expect(
      within(dialog).getByRole("heading", { name: "Historial de «main»" }),
    ).toBeVisible();
    const rows = within(
      within(dialog).getByRole("list", { name: "Entradas del historial" }),
    ).getAllByRole("button");
    expect(rows).toHaveLength(3);
    expect(rows[1]).toHaveTextContent("2222222");
    expect(rows[1]).toHaveTextContent("«Perdido»");
    expect(rows[1]).toHaveTextContent("· solo en este historial");
    expect(rows[1]).toHaveTextContent("reset: moving to HEAD~1");
    expect(rows[0]).not.toHaveTextContent("solo en este historial");
    expect(rows[0]).toHaveTextContent("hace 10 minutos");
  });

  it("says how long git keeps them, with the repository's values", async () => {
    let dialog = await openFor("refs/heads/main");
    expect(dialog).toHaveTextContent(
      "Los commits que ya no están en ninguna rama se pueden perder pasados unos 30 días: si quieres conservarlos, crea una rama.",
    );
    expect(dialog).toHaveTextContent(
      "Las demás entradas se guardan unos 90 días.",
    );

    act(() => useReflogStore.getState().close());
    ipc.branchReflog.mockResolvedValue({
      status: "ok",
      data: reflog({
        expiry: {
          reachable: { type: "never" },
          unreachable: { type: "days", days: 14 },
        },
      }),
    });
    await act(() => useReflogStore.getState().open(1, "refs/heads/main"));
    dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent("pasados unos 14 días");
    expect(dialog).toHaveTextContent("Las demás entradas no se borran nunca.");
  });

  it("says when git keeps no history", async () => {
    const dialog = await openFor("refs/heads/feature", reflog({ entries: [] }));
    expect(dialog).toHaveTextContent("Git no guarda historial de esta rama");
  });
});

describe("an entry's menu", () => {
  it("shows the commit in the graph, unless no branch has it", async () => {
    await openFor("refs/heads/main");
    let menu = await menuOf("Perdido");
    const show = within(menu).getByRole("menuitem", { name: "Ver el commit" });
    expect(show).toBeDisabled();
    expect(show).toHaveAccessibleDescription(/crea una rama aquí para verlo/);
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("dialog")).toBeVisible();

    menu = await menuOf("Cambio 1");
    const user = userEvent.setup();
    await user.click(
      within(menu).getByRole("menuitem", { name: "Ver el commit" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("1"));
  });

  it("creates a branch there with the usual dialog", async () => {
    await openFor("refs/heads/main");
    const menu = await menuOf("Perdido");
    const user = userEvent.setup();
    await user.click(
      within(menu).getByRole("menuitem", { name: "Crear una rama aquí…" }),
    );
    expect(useBranchStore.getState().dialog).toMatchObject({
      kind: "create",
      repoId: 1,
      start: oid("2"),
    });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("resets the current branch there, with the reset dialog", async () => {
    ipc.resetPreview.mockResolvedValue({
      status: "error",
      error: { kind: "cancelled", message: "", details: null },
    } as never);
    await openFor("refs/heads/main");
    const menu = await menuOf("Perdido");
    const user = userEvent.setup();
    await user.click(
      within(menu).getByRole("menuitem", { name: "Restablecer «main» aquí…" }),
    );
    expect(ipc.resetPreview).toHaveBeenCalledWith(1, oid("2"), "mixed");
  });

  it("does not reset a branch that is not the current one", async () => {
    await openFor("refs/heads/feature");
    const menu = await menuOf("Perdido");
    const reset = within(menu).getByRole("menuitem", {
      name: "Restablecer «feature» aquí…",
    });
    expect(reset).toBeDisabled();
    expect(reset).toHaveAccessibleDescription("Cámbiate antes a «feature»");
  });

  it("moves with the arrows and opens with ⇧F10", async () => {
    const dialog = await openFor("refs/heads/main");
    const rows = within(dialog).getAllByRole("button", {
      name: /Cambio|Perdido/,
    });
    rows[0]?.focus();
    fireEvent.keyDown(rows[0] as HTMLElement, { key: "ArrowDown" });
    expect(rows[1]).toHaveFocus();
    fireEvent.keyDown(rows[1] as HTMLElement, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu", { name: "Entrada 2222222" })).toBeVisible();
  });
});

describe("its dates", () => {
  afterEach(() => vi.useRealTimers());

  it("are counted from when it opens, not from when the app started", async () => {
    // The app starts on 19 November at 19:26; the history opens the next
    // day at 22:09 (what the user saw: "tomorrow", "8 minutes ago").
    const started = Date.UTC(2026, 10, 19, 18, 26);
    const opened = Date.UTC(2026, 10, 20, 21, 9, 30);
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(started);
    ipc.branchReflog.mockResolvedValue({
      status: "ok",
      data: reflog({
        entries: [
          entry(0, {
            time: opened / 1000 - 30,
            message: "rebase (finish): refs/heads/main",
          }),
          entry(1, { time: started / 1000 - 8 * 60 }),
        ],
      }),
    });
    render(<ReflogDialog />);
    vi.setSystemTime(opened);
    await act(() => useReflogStore.getState().open(1, "refs/heads/main"));
    const items = within(screen.getByRole("dialog")).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("ahora");
    expect(items[0]).not.toHaveTextContent("mañana");
    expect(items[1]).toHaveTextContent("ayer");
    expect(items[1]).not.toHaveTextContent("minutos");
  });
});
