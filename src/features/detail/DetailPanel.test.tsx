import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type { CommitDetails } from "../../ipc";
import {
  commandError,
  ipc,
  oid,
  sampleLines,
  sampleRefs,
} from "../../test/ipc";
import { useGraphStore } from "../graph/store";
import { DetailPanel } from "./DetailPanel";

const time = Math.floor(Date.now() / 1000) - 2 * 3600;

function details(overrides: Partial<CommitDetails> = {}): CommitDetails {
  const person = { name: "Ana", email: "ana@example.com", time };
  return {
    oid: oid("f"),
    parents: [oid("b"), oid("d")],
    author: person,
    committer: person,
    summary: "Merge branch 'feature'",
    body: "Brings in D and C.\n\nReviewed-by: Bea",
    ...overrides,
  };
}

function withSelection(selectedOid: string | null) {
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: sampleLines(),
        maxColumn: 1,
        refs: sampleRefs(),
        error: null,
        selectedOid,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
}

describe("DetailPanel", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("asks to select a commit when nothing is selected", () => {
    withSelection(null);
    render(<DetailPanel repoId={1} />);
    expect(
      screen.getByText("Selecciona un commit para ver sus detalles."),
    ).toBeVisible();
    expect(ipc.commitDetails).not.toHaveBeenCalled();
  });

  it("shows the selected commit's message, author, hash and parents", async () => {
    ipc.commitDetails.mockResolvedValue({ status: "ok", data: details() });
    withSelection(oid("f"));
    render(<DetailPanel repoId={1} />);

    expect(ipc.commitDetails).toHaveBeenCalledWith(1, oid("f"));
    const panel = screen.getByRole("complementary", { name: "Detalle" });
    const heading = await within(panel).findByRole("heading", {
      name: "Merge branch 'feature'",
    });
    expect(heading).toHaveClass("selectable");
    expect(within(panel).getByText(/Reviewed-by: Bea/)).toHaveClass(
      "selectable",
    );
    expect(within(panel).getByText("Ana <ana@example.com>")).toBeVisible();
    expect(within(panel).getByText(/hace 2 horas/)).toBeVisible();
    const hash = within(panel).getByText(oid("f"));
    expect(hash).toHaveClass("selectable");
    expect(hash).toHaveAttribute("title", oid("f"));
    expect(within(panel).getByText("Padres")).toBeVisible();
    // Same author and committer: no separate committer row.
    expect(within(panel).queryByText("Committer")).toBeNull();
  });

  it("re-flows the body's wrapped lines", async () => {
    ipc.commitDetails.mockResolvedValue({
      status: "ok",
      data: details({
        body: "Stream the commits instead of walking the whole\nhistory first.\n\n- one\n- two\n\nSigned-off-by: Ana <ana@example.com>",
      }),
    });
    withSelection(oid("f"));
    render(<DetailPanel repoId={1} />);
    const body = await screen.findByText(/Stream the commits/);
    expect(body.textContent).toBe(
      "Stream the commits instead of walking the whole history first.\n\n- one\n- two\n\nSigned-off-by: Ana <ana@example.com>",
    );
  });

  it("shows a different committer and a root commit", async () => {
    ipc.commitDetails.mockResolvedValue({
      status: "ok",
      data: details({
        oid: oid("a"),
        parents: [],
        summary: "A",
        body: "",
        committer: { name: "Bea", email: "bea@example.com", time },
      }),
    });
    withSelection(oid("a"));
    render(<DetailPanel repoId={1} />);
    expect(await screen.findByText("Bea <bea@example.com>")).toBeVisible();
    expect(screen.getByText("Committer")).toBeVisible();
    expect(screen.getByText("Ninguno (commit raíz)")).toBeVisible();
  });

  it("shows the commit's branch and tag labels", async () => {
    ipc.commitDetails.mockResolvedValue({
      status: "ok",
      data: details({
        oid: oid("e"),
        parents: [oid("f")],
        summary: "E",
        body: "",
      }),
    });
    withSelection(oid("e"));
    render(<DetailPanel repoId={1} />);
    expect(await screen.findByText("main")).toBeVisible();
    expect(screen.getByText("origin/main")).toBeVisible();
  });

  it("navigates to a parent and copies the hash", async () => {
    const user = userEvent.setup();
    ipc.commitDetails.mockResolvedValue({ status: "ok", data: details() });
    withSelection(oid("f"));
    render(<DetailPanel repoId={1} />);

    await user.click(await screen.findByRole("button", { name: "Copiar" }));
    expect(await navigator.clipboard.readText()).toBe(oid("f"));
    expect(screen.getByRole("button", { name: "Copiado" })).toBeVisible();

    await user.click(
      screen.getByRole("button", { name: oid("d").slice(0, 7) }),
    );
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("d"));
  });

  it("ignores a late answer for a commit that is no longer selected", async () => {
    let answerFirst:
      ((v: Awaited<ReturnType<typeof ipc.commitDetails>>) => void) | undefined;
    ipc.commitDetails
      .mockImplementationOnce(
        () => new Promise((resolve) => (answerFirst = resolve)),
      )
      .mockResolvedValueOnce({
        status: "ok",
        data: details({
          oid: oid("b"),
          parents: [oid("a")],
          summary: "B",
          body: "",
        }),
      });
    withSelection(oid("f"));
    render(<DetailPanel repoId={1} />);
    act(() => useGraphStore.getState().select(1, oid("b")));
    expect(await screen.findByRole("heading", { name: "B" })).toBeVisible();

    await act(async () => answerFirst?.({ status: "ok", data: details() }));
    expect(screen.getByRole("heading", { name: "B" })).toBeVisible();
  });

  it("explains failures with git's output", async () => {
    ipc.commitDetails.mockResolvedValue({
      status: "error",
      error: commandError("unknown", "$ git show\nfatal: bad object"),
    });
    withSelection(oid("f"));
    render(<DetailPanel repoId={1} />);
    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText("No se pudo cargar el commit."),
    ).toBeVisible();
    expect(within(alert).getByText(/fatal: bad object/)).toHaveClass(
      "selectable",
    );
  });
});
