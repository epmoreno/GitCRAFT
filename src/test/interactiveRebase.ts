import type { InteractivePreview, PlanPreview, RebasePlan } from "../ipc";
import { oid } from "./ipc";

/** Sample data for the interactive rebase's tests (task 13.4). */

/** A to D, oldest first, as the preview lists them. */
export function samplePreview(
  overrides: Partial<InteractivePreview> = {},
): InteractivePreview {
  const commit = (label: string, subject: string) => ({
    commit: { oid: oid(label), short: label.repeat(7), subject },
    message: `${subject}\n\nCuerpo de ${label}`,
    author: "Ana",
    authorTime: 1_700_000_000,
    published: false,
  });
  return {
    branch: "main",
    from: oid("a"),
    base: oid("0"),
    refused: null,
    commits: [
      commit("a", "Añade la guía"),
      commit("b", "Prueba temporal"),
      commit("c", "wip"),
      commit("d", "Arregla el tooltip"),
    ],
    publishedIn: [],
    blocked: null,
    blocking: [],
    messages: true,
    exact: true,
    gitVersion: "2.55.0",
    fingerprint: "fp-1",
    ...overrides,
  };
}

/** What the core would predict for `plan`, simplified: nothing stops,
 * nothing becomes empty. */
export function samplePrediction(
  preview: InteractivePreview,
  plan: RebasePlan,
  overrides: Partial<PlanPreview> = {},
): PlanPreview {
  const original = preview.commits.map((c) => c.commit.oid);
  const unchanged =
    plan.steps.every((s, i) => s.oid === original[i]) &&
    plan.steps.every(
      (s) =>
        s.action === "pick" ||
        (s.action === "reword" &&
          s.message ===
            preview.commits.find((c) => c.commit.oid === s.oid)?.message),
    );
  const firstChanged = plan.steps.findIndex(
    (s, i) => s.oid !== original[i] || s.action !== "pick",
  );
  const dropped = plan.steps
    .filter((s) => s.action === "drop")
    .map((s) => preview.commits.find((c) => c.commit.oid === s.oid)!.commit);
  return {
    problem: unchanged ? { kind: "unchanged" } : null,
    dropped,
    empty: [],
    stopsAt: null,
    predicted: true,
    commitsAfter: plan.steps.filter(
      (s) => s.action === "pick" || s.action === "reword",
    ).length,
    rewritten: unchanged ? 0 : plan.steps.length - Math.max(0, firstChanged),
    publishedIn: [],
    ...overrides,
  };
}
