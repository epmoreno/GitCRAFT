import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import type {
  ConflictFile,
  ConflictSides,
  Segment,
  WorkingStatus,
} from "../../ipc";
import { setPlatformForTests } from "../../app/platform";
import { cleanStatus, ipc } from "../../test/ipc";
import { useRepoStore } from "../repo/store";
import { AbortDialog } from "../remote/RemoteDialogs";
import { useRemoteStore } from "../remote/store";
import { WorkingDirPanel } from "../workdir/WorkingDirPanel";
import { useWorkdirStore } from "../workdir/store";
import { ConflictView } from "./ConflictView";
import { block } from "../../test/conflicts";
import { useConflictStore } from "./store";
import { UnsavedDialog } from "./UnsavedDialog";

const commit = (short: string, subject = "subject") => ({
  oid: short.padEnd(40, "0"),
  short,
  subject,
});

const merge: ConflictSides = {
  type: "merge",
  branch: "main",
  head: commit("aaaaaaa"),
  merging: { commit: commit("bbbbbbb"), branches: ["feature"] },
};

function textFile(segments: Segment[], fingerprint = "fp1"): ConflictFile {
  const version = { oid: "1".repeat(40), kind: "file" as const, size: 10 };
  return {
    path: "src/main.rs",
    kind: "bothModified",
    base: version,
    ours: version,
    theirs: version,
    fingerprint,
    content: { type: "blocks", segments, markerSize: 7 },
  };
}

const twoBlocks: Segment[] = [
  { type: "common", text: "fn main() {\n" },
  {
    type: "conflict",
    block: block("    a = 100;\n", "    a = 10;\n", { base: "    a = 1;\n" }),
  },
  { type: "common", text: "    b = 2;\n" },
  { type: "conflict", block: block("// main\n", "// feature\n") },
  { type: "common", text: "}\n" },
];

function serve(file: ConflictFile, sides: ConflictSides | null = merge) {
  ipc.conflictFile.mockResolvedValue({ status: "ok", data: file });
  ipc.conflictSides.mockResolvedValue({ status: "ok", data: sides });
  ipc.writeResolution.mockResolvedValue({ status: "ok", data: "fp2" });
  ipc.markResolved.mockResolvedValue({
    status: "ok",
    data: { type: "resolved" },
  });
}

async function open() {
  const view = render(<ConflictView repoId={1} path="src/main.rs" />);
  await screen.findByText(/Bloque 1 de 2/);
  return view;
}

const blockCards = () => screen.getAllByLabelText(/^Bloque \d de 2$/);

beforeEach(async () => {
  await i18n.changeLanguage("es");
  setPlatformForTests("mac");
});

describe("the conflict view", () => {
  it("names the buttons after each side", async () => {
    serve(textFile(twoBlocks));
    await open();
    const [first] = blockCards();
    for (const name of [
      "Usar «main»",
      "Usar «feature»",
      "Ambas («main» primero)",
      "Ambas («feature» primero)",
    ]) {
      expect(within(first!).getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("cuts long names and keeps the whole one for screen readers", async () => {
    const long = "feature/a-very-long-branch-name-indeed";
    serve(textFile(twoBlocks), {
      ...merge,
      merging: { commit: commit("bbbbbbb"), branches: [long] },
    });
    await open();
    const [first] = blockCards();
    const button = within(first!).getByRole("button", {
      name: `Usar «${long}»`,
    });
    expect(button).toHaveTextContent(/^Usar «feature\/a-very-long-bra…»$/);
  });

  it("saves the chosen result byte for byte, keeping markers of the rest", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks));
    await open();
    const [first] = blockCards();
    await user.click(
      within(first!).getByRole("button", { name: "Usar «feature»" }),
    );
    expect(screen.getByText(/elegidos: 1/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    expect(ipc.writeResolution).toHaveBeenCalledWith(
      1,
      "src/main.rs",
      "fn main() {\n    a = 10;\n    b = 2;\n<<<<<<< HEAD\n// main\n=======\n// feature\n>>>>>>> feature\n}\n",
      "fp1",
    );
    expect(ipc.markResolved).not.toHaveBeenCalled();
  });

  it("keeps CRLF line endings", async () => {
    const user = userEvent.setup();
    const crlf: Segment[] = [
      { type: "common", text: "x\r\n" },
      { type: "conflict", block: block("a\r\n", "b\r\n") },
    ];
    serve(textFile(crlf));
    render(<ConflictView repoId={1} path="src/main.rs" />);
    await screen.findByText(/Bloque 1 de 1/);
    await user.click(
      screen.getByRole("button", { name: "Ambas («main» primero)" }),
    );
    await user.click(screen.getByRole("button", { name: "Guardar" }));
    expect(ipc.writeResolution.mock.calls[0]?.[2]).toBe("x\r\na\r\nb\r\n");
  });

  it("asks before marking a file that keeps markers, showing them", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks));
    ipc.markResolved.mockResolvedValueOnce({
      status: "ok",
      data: {
        type: "markersLeft",
        lines: [
          { line: 4, text: "<<<<<<< HEAD" },
          { line: 8, text: ">>>>>>> feature" },
        ],
      },
    });
    await open();
    await user.click(
      screen.getByRole("button", { name: "Guardar y marcar como resuelto" }),
    );
    const dialog = await screen.findByRole("alertdialog", {
      name: "Quedan marcas de conflicto",
    });
    expect(dialog).toHaveTextContent("línea 4<<<<<<< HEAD");
    expect(dialog).toHaveTextContent("línea 8>>>>>>> feature");
    expect(ipc.markResolved).toHaveBeenLastCalledWith(1, "src/main.rs", false);
    await user.click(
      within(dialog).getByRole("button", { name: "Marcar igualmente" }),
    );
    expect(ipc.markResolved).toHaveBeenLastCalledWith(1, "src/main.rs", true);
  });

  it("shows the base only when asked", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks));
    await open();
    const [first, second] = blockCards();
    expect(screen.queryByText("Base (ancestro común)")).toBeNull();
    await user.click(
      within(first!).getByRole("button", { name: "Mostrar base" }),
    );
    expect(
      within(first!).getByText("Base (ancestro común)"),
    ).toBeInTheDocument();
    expect(
      within(first!).getByText("a = 1;", { exact: false }),
    ).toBeInTheDocument();
    // The second block's base is unknown: its button says so.
    expect(
      within(second!).getByRole("button", { name: "Mostrar base" }),
    ).toBeDisabled();
  });

  it("keeps unsaved choices when the view closes and opens again", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks));
    const view = await open();
    await user.click(
      within(blockCards()[0]!).getByRole("button", { name: "Usar «main»" }),
    );
    view.unmount();
    await open();
    expect(
      within(blockCards()[0]!).getByRole("button", { name: "Usar «main»" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText(/elegidos: 1/)).toBeInTheDocument();
  });

  it("warns, and writes nothing, when the file changed after choosing", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks, "fp-old"));
    const view = await open();
    await user.click(
      within(blockCards()[0]!).getByRole("button", { name: "Usar «main»" }),
    );
    view.unmount();
    serve(textFile(twoBlocks, "fp-edited-elsewhere"));
    await open();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /ha cambiado fuera de GitCRAFT/,
    );
    expect(screen.getByRole("button", { name: "Guardar" })).toBeDisabled();
    await user.click(
      screen.getByRole("button", { name: "Descartar mis elecciones" }),
    );
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("moves between blocks with ⌥↓ / ⌥↑", async () => {
    const user = userEvent.setup();
    serve(textFile(twoBlocks));
    await open();
    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(screen.getByText(/Bloque 2 de 2/)).toBeInTheDocument();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(screen.getByText(/Bloque 1 de 2/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Bloque siguiente (⌥↓)" }),
    ).toBeInTheDocument();
  });

  it("explains the sides of a rebase every time", async () => {
    serve(textFile(twoBlocks), {
      type: "rebase",
      branch: "topic",
      onto: { commit: commit("ccccccc"), branches: ["main"] },
      head: commit("ccccccc"),
      applying: commit("ddddddd", "topic one"),
      progress: { current: 1, total: 3 },
    });
    await open();
    expect(screen.getByRole("note")).toHaveTextContent(
      /a la izquierda está «main».*tu commit de «topic»/,
    );
    expect(
      within(blockCards()[0]!).getByRole("button", { name: "Usar «topic»" }),
    ).toBeInTheDocument();
  });

  it("resolves a binary file by keeping one version", async () => {
    const user = userEvent.setup();
    serve({
      ...textFile([]),
      path: "img.png",
      content: { type: "wholeFile", reason: { type: "binary" } },
    });
    ipc.chooseVersion.mockResolvedValue({ status: "ok", data: "fp9" });
    render(<ConflictView repoId={1} path="img.png" />);
    await user.click(
      await screen.findByRole("button", { name: "Usar «feature»" }),
    );
    expect(ipc.chooseVersion).toHaveBeenCalledWith(
      1,
      "img.png",
      "theirs",
      "fp1",
    );
    expect(ipc.markResolved).toHaveBeenCalledWith(1, "img.png", false);
  });

  it("deletes a file one side deleted only after confirming", async () => {
    const user = userEvent.setup();
    serve({
      ...textFile([]),
      kind: "deletedByThem",
      theirs: null,
      content: { type: "wholeFile", reason: { type: "oneSideMissing" } },
    });
    ipc.resolveByDeleting.mockResolvedValue({ status: "ok", data: null });
    render(<ConflictView repoId={1} path="src/main.rs" />);
    expect(await screen.findByText("eliminado")).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Eliminar el archivo…" }),
    );
    expect(ipc.resolveByDeleting).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Eliminar" }));
    expect(ipc.resolveByDeleting).toHaveBeenCalledWith(1, "src/main.rs", "fp1");
  });
});

describe("unsaved choices outside the view", () => {
  const conflicted = (): WorkingStatus => ({
    ...cleanStatus(),
    inProgress: "merge",
    conflicted: [{ path: "src/main.rs", kind: "bothModified" }],
  });

  function withDraft() {
    useConflictStore
      .getState()
      .setChoice(1, "src/main.rs", "fp1", 0, { kind: "ours" });
  }

  it("marks the file in the conflicts list", () => {
    withDraft();
    useWorkdirStore.setState({
      statuses: { 1: { status: conflicted(), error: null } },
    });
    render(<WorkingDirPanel repoId={1} />);
    const conflicts = screen.getByRole("region", { name: "Conflictos" });
    expect(
      within(conflicts).getByRole("img", {
        name: "Tiene elecciones sin guardar",
      }),
    ).toBeInTheDocument();
  });

  it("asks before closing the tab, listing the files", async () => {
    const user = userEvent.setup();
    useRepoStore.setState({
      tabs: [
        {
          id: 1,
          info: {
            path: "/src/sample",
            name: "sample",
            gitDir: "/src/sample/.git",
            commonDir: "/src/sample/.git",
          },
        },
      ],
      activeId: 1,
    } as never);
    withDraft();
    render(<UnsavedDialog />);
    await act(() => useRepoStore.getState().closeTab(1));
    expect(ipc.closeRepository).not.toHaveBeenCalled();
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("src/main.rs");
    await user.click(
      within(dialog).getByRole("button", { name: "Cerrar la pestaña" }),
    );
    expect(ipc.closeRepository).toHaveBeenCalledWith(1);
    expect(useConflictStore.getState().drafts[1]).toBeUndefined();
  });

  it("lists them when aborting", () => {
    withDraft();
    useRemoteStore.setState({
      dialog: { kind: "abort", repoId: 1, operation: "merge" },
    });
    render(<AbortDialog />);
    expect(screen.getByRole("alertdialog")).toHaveTextContent(
      /se perderán las elecciones sin guardar en:src\/main.rs/,
    );
  });

  it("forgets them when the operation ends", () => {
    withDraft();
    useWorkdirStore.setState({
      statuses: { 1: { status: cleanStatus(), error: null } },
    });
    expect(useConflictStore.getState().drafts[1]).toBeUndefined();
  });
});

describe("files marked resolved", () => {
  it("can be put back in conflict after confirming", async () => {
    const user = userEvent.setup();
    ipc.resolvedFiles.mockResolvedValue({
      status: "ok",
      data: [{ path: "a.txt", changedSince: true }],
    });
    ipc.unmarkResolved.mockResolvedValue({ status: "ok", data: null });
    useWorkdirStore.setState({
      statuses: {
        1: { status: { ...cleanStatus(), inProgress: "merge" }, error: null },
      },
    });
    render(<WorkingDirPanel repoId={1} />);
    const resolved = await screen.findByRole("region", { name: "Resueltos" });
    await user.click(
      within(resolved).getByRole("button", {
        name: "Volver a marcar como en conflicto…",
      }),
    );
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toHaveTextContent(/se olvida la versión que marcaste/);
    await user.click(
      within(dialog).getByRole("button", { name: "Volver a marcar" }),
    );
    expect(ipc.unmarkResolved).toHaveBeenCalledWith(1, "a.txt");
  });
});
