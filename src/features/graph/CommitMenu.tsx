import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CommitActions } from "../../ipc";
import { copyText } from "../../ui/clipboard";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { runCommitAction } from "../history/store";
import { mergeMenuItem } from "../merge/menu";
import type { RefLabel } from "./refLabels";
import { useGraphStore } from "./store";
import {
  availability,
  DANGEROUS_ACTIONS,
  READY_ACTIONS,
  type CommitAction,
} from "./commitActions";

/** The actions on one commit of the graph, at (`x`, `y`). Only actions that
 * already work are listed; those that cannot be done on this commit are
 * disabled with the reason as their tooltip. */
export function CommitMenu({
  repoId,
  oid,
  x,
  y,
  branches,
  onClose,
}: {
  repoId: number;
  oid: string;
  x: number;
  y: number;
  /** The labels on this commit: its branches get "Merge…" here too, so
   * the keyboard reaches them. */
  branches?: RefLabel[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const refs = useGraphStore((s) => s.graphs[repoId]?.refs ?? null);
  const [facts, setFacts] = useState<CommitActions | null>(null);
  const needsFacts = READY_ACTIONS.length > 0;

  useEffect(() => {
    if (!needsFacts) return;
    let stale = false;
    void commands.commitActions(repoId, oid).then((result) => {
      if (!stale && result.status === "ok") setFacts(result.data);
    });
    return () => {
      stale = true;
    };
  }, [repoId, oid, needsFacts]);

  const actionItem = (action: CommitAction): MenuItem => {
    const branch = facts?.branch ?? "";
    const state = facts ? availability(action, facts) : null;
    return {
      label: t(`commitMenu.action.${action}`, { branch }),
      danger: DANGEROUS_ACTIONS.has(action),
      separatorBefore: DANGEROUS_ACTIONS.has(action),
      disabled: !state?.enabled,
      hint:
        state && !state.enabled
          ? t(`commitMenu.why.${state.reason}`, { branch })
          : undefined,
      onSelect: () => runCommitAction(action, repoId, oid),
    };
  };

  const mergeItems: MenuItem[] = (branches ?? [])
    .filter((b) => (b.kind === "local" || b.kind === "remote") && !b.current)
    .map((b, index) => ({
      ...mergeMenuItem(t, repoId, refs, { name: b.refname, shortName: b.text }),
      separatorBefore: index === 0,
    }));

  const items: MenuItem[] = [
    ...READY_ACTIONS.map(actionItem),
    ...mergeItems,
    {
      label: t("commitMenu.copyHash"),
      separatorBefore: READY_ACTIONS.length > 0,
      onSelect: () => void copyText(oid),
    },
  ];

  return (
    <ContextMenu
      x={x}
      y={y}
      label={t("commitMenu.label", { short: oid.slice(0, 7) })}
      items={items}
      onClose={onClose}
    />
  );
}
