import { commands } from "../ipc";

/**
 * What the interface needs from the system's web engine (WKWebView,
 * WebView2 or WebKitGTK; see "Motores web" in docs/ARCHITECTURE.md), checked
 * in the running app. Returns the problems found, in English, for the CI log.
 */
export function engineProblems(): string[] {
  const problems: string[] = [];
  const supports = (property: string, value: string) =>
    typeof CSS !== "undefined" &&
    typeof CSS.supports === "function" &&
    CSS.supports(property, value);
  if (!supports("color", "color-mix(in srgb, red 50%, blue)")) {
    problems.push("color-mix() is not supported (tints would be missing)");
  }
  if (
    !supports("user-select", "none") &&
    !supports("-webkit-user-select", "none")
  ) {
    problems.push("user-select is not supported");
  }
  if (!document.createElement("canvas").getContext("2d")) {
    problems.push("there is no 2D canvas (the graph cannot be drawn)");
  }
  const tokens = getComputedStyle(document.documentElement)
    .getPropertyValue("--font-ui")
    .trim();
  if (tokens === "")
    problems.push("the design tokens (tokens.css) did not load");
  if (!document.querySelector('[role="main"]')) {
    problems.push("the interface did not render");
  }
  return problems;
}

/** In a smoke run (`--smoke`, used by the CI), checks the engine once the
 * interface has rendered and reports, which ends the run. */
export async function reportIfSmokeRun(): Promise<void> {
  if (!(await commands.smokeMode())) return;
  // After the first paint, so the rendered interface is there to check.
  await new Promise((resolve) => requestAnimationFrame(resolve));
  await commands.smokeReport(engineProblems());
}
