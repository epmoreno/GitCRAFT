import { frameOrTick, settles } from "./settle";

describe("waiting in the smoke run", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("holds as soon as the condition does", async () => {
    let checks = 0;
    const settled = await settles(() => ++checks === 3);
    expect(settled.ok).toBe(true);
    expect(checks).toBe(3);
  });

  it("gives up after the limit, even if the engine gives no frames", async () => {
    vi.stubGlobal("requestAnimationFrame", () => 0);
    const settled = await settles(() => false, 120);
    expect(settled.ok).toBe(false);
    expect(settled.ms).toBeGreaterThan(120);
  });

  it("says whether a frame came", async () => {
    vi.stubGlobal("requestAnimationFrame", (cb: () => void) => cb());
    expect(await frameOrTick()).toBe("frame");
    vi.stubGlobal("requestAnimationFrame", () => 0);
    expect(await frameOrTick()).toBe("tick");
  });
});
