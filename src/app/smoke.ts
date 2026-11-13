import { useConflictStore } from "../features/conflicts/store";
import { commands } from "../ipc";
import { confirmClosing, installWindowCloseGuard, requestQuit } from "./quit";

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
  if (!document.querySelector("main")) {
    problems.push("the interface did not render");
  }
  return problems;
}

/** In a smoke run (`--smoke`, used by the CI), checks the engine once the
 * interface has rendered and reports; then quits the way the user does and
 * the app must end by itself (see src-tauri/src/smoke.rs). */
export async function reportIfSmokeRun(): Promise<void> {
  if (!(await commands.smokeMode())) return;
  // After the first paint, so the rendered interface is there to check.
  await new Promise((resolve) => requestAnimationFrame(resolve));
  const problems = engineProblems();
  await commands.smokeReport(problems);
  if (problems.length > 0) return;
  try {
    await quitLikeTheUser();
  } catch (error) {
    await commands.smokeReport([`quitting failed: ${String(error)}`]);
  }
}

/** ⌘Q with an unsaved conflict choice, then "Quit" in the warning: the
 * longest way out, through the window-close guard. */
export async function quitLikeTheUser(): Promise<void> {
  await installWindowCloseGuard();
  useConflictStore
    .getState()
    .setChoice(-1, "smoke-check.txt", null, 0, { kind: "ours" });
  await requestQuit();
  if (useConflictStore.getState().closing?.kind !== "quit") {
    throw new Error("Quit did not ask about the unsaved choice");
  }
  await confirmClosing();
}
