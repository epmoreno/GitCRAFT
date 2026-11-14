/**
 * Which actions a commit of the graph offers, and why one is not
 * available. Pure: the graph's context menu and the detail panel share it.
 *
 * An action appears only once its task works (`READY_ACTIONS`): never a
 * button that does nothing.
 */

import type { CommitActions } from "../../ipc";

export type CommitAction = "cherryPick" | "revert" | "reset" | "createTag";

/** The actions that already work, in menu order. Each task of phase 2
 * (10.2 cherry-pick, 10.3 revert, 10.4 reset, 10.5 tags) adds its own. */
export const READY_ACTIONS: readonly CommitAction[] = [
  "cherryPick",
  "revert",
  "reset",
];

/** Shown apart, with the danger style (like "Discard"). */
export const DANGEROUS_ACTIONS: ReadonlySet<CommitAction> = new Set(["reset"]);

export type Availability =
  | { enabled: true }
  | {
      enabled: false;
      /** i18n key under `commitMenu.why`. */
      reason:
        | "inProgress"
        | "detached"
        | "merge"
        | "alreadyInBranch"
        | "notInBranch"
        | "isHead";
    };

const ok: Availability = { enabled: true };
const no = (reason: Exclude<Availability, { enabled: true }>["reason"]) =>
  ({ enabled: false, reason }) as const;

/** Whether `action` can be done on the commit `facts` describe. */
export function availability(
  action: CommitAction,
  facts: CommitActions,
): Availability {
  if (action === "createTag") return ok;
  // The rest change the current branch.
  if (facts.inProgress) return no("inProgress");
  if (facts.branch === null) return no("detached");
  switch (action) {
    case "cherryPick":
      if (facts.isMerge) return no("merge");
      return facts.inBranch ? no("alreadyInBranch") : ok;
    case "revert":
      if (facts.isMerge) return no("merge");
      return facts.inBranch ? ok : no("notInBranch");
    case "reset":
      return facts.isHead ? no("isHead") : ok;
  }
}
