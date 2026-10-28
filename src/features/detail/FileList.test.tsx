import { render, screen, within } from "@testing-library/react";
import i18n from "../../i18n";
import type { FileChange } from "../../ipc";
import { commandError, ipc, oid } from "../../test/ipc";
import { FileList } from "./FileList";

const file = (overrides: Partial<FileChange>): FileChange => ({
  path: "src/app.ts",
  oldPath: null,
  status: "modified",
  similarity: null,
  additions: 3,
  deletions: 1,
  ...overrides,
});

describe("FileList", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("lists files with status, path and line counts", async () => {
    ipc.commitChanges.mockResolvedValue({
      status: "ok",
      data: {
        base: oid("b"),
        files: [
          file({}),
          file({
            path: "docs/new.md",
            status: "added",
            additions: 10,
            deletions: 0,
          }),
          file({
            path: "lib/renamed.rs",
            oldPath: "lib/old.rs",
            status: "renamed",
            similarity: 90,
            additions: 1,
            deletions: 1,
          }),
          file({
            path: "logo.png",
            status: "added",
            additions: null,
            deletions: null,
          }),
        ],
      },
    });
    render(<FileList repoId={1} oid={oid("e")} isMerge={false} />);

    const list = await screen.findByRole("region", {
      name: "Archivos cambiados",
    });
    expect(ipc.commitChanges).toHaveBeenCalledWith(1, oid("e"));
    expect(within(list).getByRole("heading")).toHaveTextContent(
      "4 archivos cambiados+14−2",
    );
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(4);
    expect(within(rows[0]!).getByLabelText("Modificado")).toHaveTextContent(
      "M",
    );
    expect(rows[0]).toHaveTextContent("src/app.ts+3−1");
    expect(within(rows[2]!).getByLabelText("Renombrado")).toHaveTextContent(
      "R",
    );
    expect(rows[2]).toHaveTextContent("antes: lib/old.rs");
    expect(rows[3]).toHaveTextContent("binario");
    expect(within(list).queryByText(/primer padre/)).toBeNull();
  });

  it("says merges are shown against their first parent", async () => {
    ipc.commitChanges.mockResolvedValue({
      status: "ok",
      data: { base: oid("b"), files: [file({})] },
    });
    render(<FileList repoId={1} oid={oid("f")} isMerge />);
    expect(
      await screen.findByText(
        `Cambios respecto al primer padre (${oid("b").slice(0, 7)}).`,
      ),
    ).toBeVisible();
  });

  it("handles commits without changes and errors", async () => {
    const { unmount } = render(
      <FileList repoId={1} oid={oid("a")} isMerge={false} />,
    );
    expect(
      await screen.findByText("Este commit no cambia ningún archivo."),
    ).toBeVisible();
    unmount();

    ipc.commitChanges.mockResolvedValue({
      status: "error",
      error: commandError("unknown", "$ git diff-tree\nfatal: bad object"),
    });
    render(<FileList repoId={1} oid={oid("c")} isMerge={false} />);
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText(/fatal: bad object/)).toHaveClass(
      "selectable",
    );
  });
});
