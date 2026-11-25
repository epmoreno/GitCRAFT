import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import conf from "../../src-tauri/tauri.conf.json";
import i18n from "../i18n";
import { emptyRefs, oid, opened } from "../test/ipc";
import { useGraphStore } from "../features/graph/store";
import { useOperationsStore } from "../features/operations/store";
import { useRepoStore } from "../features/repo/store";
import { useSettingsStore } from "../features/settings/store";
import { NARROWEST } from "./smokeNarrow";
import { Toolbar } from "./Toolbar";
import { GIVING_WAY } from "./useToolbarFit";

/** jsdom lays nothing out: the toolbar "fits" once `count` parts are gone. */
function fitsAfter(count: number) {
  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockImplementation(
    function (this: HTMLElement) {
      const gone = this.closest("header")?.dataset.dropped ?? "";
      const parts = gone.split(" ").filter((p) => p && p !== "menu");
      return parts.length >= count ? 100 : 5000;
    },
  );
}

function renderToolbar() {
  useRepoStore.setState({ tabs: [opened(1, "/src/sample")], activeId: 1 });
  useGraphStore.setState({
    graphs: {
      1: {
        status: "done",
        lines: [],
        maxColumn: 0,
        refs: {
          ...emptyRefs(),
          head: { type: "branch", name: "main", oid: oid("a") },
        },
        error: null,
        selectedOid: null,
        suggestOptimize: false,
        openFile: null,
        loadId: 1,
      },
    },
  });
  render(<Toolbar />);
  return screen.getByRole("banner");
}

async function menuLabels(bar: HTMLElement) {
  await userEvent.click(
    within(bar).getByRole("button", { name: "Más acciones" }),
  );
  return within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((item) => item.textContent);
}

describe("the toolbar in a narrow window", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("drops nothing when it fits", () => {
    fitsAfter(0);
    expect(renderToolbar().dataset.dropped).toBe("");
  });

  it("first gives up the name and the branch's room, with no menu", () => {
    fitsAfter(2);
    expect(renderToolbar().dataset.dropped).toBe("brand branchName");
  });

  it("then moves actions to the menu, in order, Settings never", async () => {
    fitsAfter(6);
    const bar = renderToolbar();
    expect(bar.dataset.dropped).toBe(
      "brand branchName publish stash branch operations menu",
    );
    expect(await menuLabels(bar)).toEqual([
      "Publicar en GitHub",
      "Stash",
      "Rama",
      "Operaciones",
    ]);
    const before = useOperationsStore.getState().open;
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Operaciones" }),
    );
    expect(useOperationsStore.getState().open).toBe(!before);
    await userEvent.click(within(bar).getByRole("button", { name: "Ajustes" }));
    expect(useSettingsStore.getState().open).toBe(true);
  });

  it("ends with Undo and Redo in the menu too, whatever the width", async () => {
    fitsAfter(99);
    const bar = renderToolbar();
    expect(bar.dataset.dropped).toBe([...GIVING_WAY, "menu"].join(" "));
    expect(await menuLabels(bar)).toEqual([
      "Publicar en GitHub",
      "Stash",
      "Rama",
      "Operaciones",
      "Deshacer",
      "Rehacer",
    ]);
  });

  it("is checked in the smoke run at the window's real minimum", () => {
    const [window] = conf.app.windows;
    expect({ width: window?.minWidth, height: window?.minHeight }).toEqual(
      NARROWEST,
    );
  });
});
