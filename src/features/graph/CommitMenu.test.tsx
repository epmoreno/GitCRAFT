import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "../../i18n";
import { setPlatformForTests } from "../../app/platform";
import {
  ipc,
  oid,
  sampleLines,
  sampleRefs,
  streamEvents,
} from "../../test/ipc";
import { GraphView } from "./GraphView";
import { useGraphStore } from "./store";

const info = {
  path: "/src/sample",
  name: "sample",
  gitDir: "/src/sample/.git",
  commonDir: "/src/sample/.git",
};

async function rows() {
  ipc.getRefs.mockResolvedValue({ status: "ok", data: sampleRefs() });
  streamEvents([
    { type: "page", lines: sampleLines() },
    { type: "done", total: 6, suggestOptimize: false },
  ]);
  render(<GraphView repoId={1} info={info} />);
  const list = await screen.findByRole("listbox", {
    name: "Historial de sample",
  });
  return { list, rows: await within(list).findAllByRole("option") };
}

beforeEach(async () => {
  await i18n.changeLanguage("es");
  setPlatformForTests("mac");
});

describe("the menu of a commit", () => {
  it("opens with a right click, selecting the commit", async () => {
    const { rows: all } = await rows();
    const tip = all[0]!;
    fireEvent.contextMenu(tip, { clientX: 40, clientY: 50 });
    const menu = screen.getByRole("menu", {
      name: `Acciones del commit ${oid("e").slice(0, 7)}`,
    });
    expect(useGraphStore.getState().graphs[1]?.selectedOid).toBe(oid("e"));
    // In 10.1 only what already works: no button that does nothing.
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent),
    ).toEqual(["Copiar hash"]);
  });

  it("copies the whole hash", async () => {
    const user = userEvent.setup();
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue();
    const { rows: all } = await rows();
    fireEvent.contextMenu(all[0]!, { clientX: 40, clientY: 50 });
    await user.click(screen.getByRole("menuitem", { name: "Copiar hash" }));
    expect(writeText).toHaveBeenCalledWith(oid("e"));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens from the keyboard with ⇧F10 or the menu key", async () => {
    const user = userEvent.setup();
    const { list, rows: all } = await rows();
    await user.click(all[2]!);
    list.focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    list.focus();
    await user.keyboard("{ContextMenu}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("does not open from the keyboard with nothing selected", async () => {
    const user = userEvent.setup();
    const { list } = await rows();
    list.focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
