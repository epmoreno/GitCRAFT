/* Waiting for the real web engine in the smoke run (src/app/smoke.ts and
 * smokeLayout.ts): a slow CI machine may need more than one frame to lay
 * out, render or scroll. */

/** The next frame, or 50 ms if the engine gives none (a hidden window or a
 * sleeping display get no frames), so a wait always ends. Says which. */
export function frameOrTick(): Promise<"frame" | "tick"> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve("frame"));
    setTimeout(() => resolve("tick"), 50);
  });
}

export interface Settled {
  ok: boolean;
  /** How long it took to hold, or to give up. */
  ms: number;
}

/** Checks once per frame until `ok` holds or `limit` ms have gone by. */
export async function settles(
  ok: () => boolean,
  limit = 2000,
): Promise<Settled> {
  const start = performance.now();
  for (;;) {
    const holds = ok();
    const ms = Math.round(performance.now() - start);
    if (holds || ms > limit) return { ok: holds, ms };
    await frameOrTick();
  }
}
