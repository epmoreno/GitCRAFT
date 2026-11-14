import type { TFunction } from "i18next";
import type { RepoRefs } from "../../ipc";
import type { MenuItem } from "../../ui/ContextMenu";
import { operationWaiting } from "../history/store";
import { useMergeStore } from "./store";

/** A branch as the menus know it. */
export type BranchName = {
  /** Full ref name (`refs/heads/…`, `refs/remotes/…`). */
  name: string;
  /** As shown (`feature`, `origin/main`). */
  shortName: string;
};

/** Why the current branch cannot be merged with or rebased onto `branch`
 * from a menu, or `null`. */
function unavailable(
  repoId: number,
  current: string | null,
  branch: BranchName,
): "detached" | "isCurrent" | "inProgress" | null {
  if (!current) return "detached";
  if (branch.name === `refs/heads/${current}`) return "isCurrent";
  return operationWaiting(repoId) ? "inProgress" : null;
}

/**
 * The entries of a branch's menu (sidebar, graph labels, commit menu):
 * "Merge «branch» into «current»…" and "Rebase «current» onto «branch»…".
 * Disabled, with the reason, when there is no current branch, when it is
 * the current branch, or while an operation waits; the dialog says the
 * rest (up to date, unrelated…).
 */
export function branchMenuItems(
  t: TFunction,
  repoId: number,
  refs: RepoRefs | null,
  branch: BranchName,
): MenuItem[] {
  const current = refs?.head.type === "branch" ? refs.head.name : null;
  const why = unavailable(repoId, current, branch);
  const common = {
    disabled: why !== null,
    hint: why ? t(`merge.menuWhy.${why}`) : undefined,
  };
  const head = current ? `refs/heads/${current}` : null;
  return [
    {
      ...common,
      label: t("merge.menu", {
        source: branch.shortName,
        target: current ?? "HEAD",
      }),
      onSelect: () => {
        if (head) {
          void useMergeStore.getState().requestMerge(repoId, branch.name, head);
        }
      },
    },
    {
      ...common,
      label: t("rebase.menu", {
        source: current ?? "HEAD",
        target: branch.shortName,
      }),
      onSelect: () => {
        if (head) {
          void useMergeStore
            .getState()
            .requestRebase(repoId, head, branch.name);
        }
      },
    },
  ];
}
