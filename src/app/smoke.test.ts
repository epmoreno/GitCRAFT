import { engineProblems } from "./smoke";

describe("smoke run checks", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;

  beforeEach(() => {
    document.documentElement.style.setProperty("--font-ui", "system-ui");
    document.body.innerHTML = "<main></main>";
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

describe("smoke run, quitting", () => {
  it("reports, then quits through the warning and closes the window", async () => {
    const { commands } = await import("../ipc");
    const { closeWindow, guardWindowClose } = await import("./windowClose");
    const { reportIfSmokeRun } = await import("./smoke");
    const { useConflictStore } = await import("../features/conflicts/store");
    vi.stubGlobal("CSS", { supports: () => true });
    document.documentElement.style.setProperty("--font-ui", "system-ui");
    document.body.innerHTML = "<main></main>";
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext =
      (() => ({})) as unknown as HTMLCanvasElement["getContext"];
    onTestFinished(() => {
      HTMLCanvasElement.prototype.getContext = getContext;
      document.documentElement.style.removeProperty("--font-ui");
      document.body.innerHTML = "";
      vi.unstubAllGlobals();
    });
    vi.stubGlobal("requestAnimationFrame", (cb: () => void) => cb());
    vi.mocked(commands.smokeMode).mockResolvedValue(true);
    vi.mocked(commands.smokeReport).mockResolvedValue(undefined);

    await reportIfSmokeRun();

    expect(commands.smokeReport).toHaveBeenCalledWith([]);
    expect(guardWindowClose).toHaveBeenCalled();
    expect(closeWindow).toHaveBeenCalled();
    expect(useConflictStore.getState().drafts).toEqual({});
  });
});
