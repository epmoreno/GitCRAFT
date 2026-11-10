import { engineProblems } from "./smoke";

describe("smoke run checks", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;

  beforeEach(() => {
    document.documentElement.style.setProperty("--font-ui", "system-ui");
    document.body.innerHTML = '<main role="main"></main>';
  });

  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = getContext;
    document.documentElement.style.removeProperty("--font-ui");
    document.body.innerHTML = "";
    vi.unstubAllGlobals();
  });

  it("finds nothing missing in a complete engine", () => {
    vi.stubGlobal("CSS", { supports: () => true });
    HTMLCanvasElement.prototype.getContext =
      (() => ({})) as unknown as HTMLCanvasElement["getContext"];
    expect(engineProblems()).toEqual([]);
  });

  it("names each thing that is missing", () => {
    vi.stubGlobal("CSS", { supports: () => false });
    document.documentElement.style.removeProperty("--font-ui");
    document.body.innerHTML = "";
    expect(engineProblems()).toEqual([
      "color-mix() is not supported (tints would be missing)",
      "user-select is not supported",
      "there is no 2D canvas (the graph cannot be drawn)",
      "the design tokens (tokens.css) did not load",
      "the interface did not render",
    ]);
  });
});
