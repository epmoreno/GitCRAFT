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

/**
 * "Merge «branch» into «current»…" for a branch's menu (sidebar, graph
 * labels, commit menu). Disabled, with the reason, when there is no current
 * branch, when it is the current branch, or while an operation waits; the
 * dialog says the rest (up to date, unrelated…).
 */
export function mergeMenuItem(
  t: TFunction,
  repoId: number,
  refs: RepoRefs | null,
  branch: BranchName,
): MenuItem {
  const current = refs?.head.type === "branch" ? refs.head.name : null;
  const why = !current
    ? "detached"
    : branch.name === `refs/heads/${current}`
      ? "isCurrent"
      : operationWaiting(repoId)
        ? "inProgress"
        : null;
  return {
    label: t("merge.menu", {
      source: branch.shortName,
      target: current ?? "HEAD",
    }),
    disabled: why !== null,
    hint: why ? t(`merge.menuWhy.${why}`) : undefined,
    onSelect: () => {
      if (current) {
        void useMergeStore
          .getState()
          .requestMerge(repoId, branch.name, `refs/heads/${current}`);
      }
    },
  };
}
