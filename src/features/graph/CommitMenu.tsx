import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CommitActions } from "../../ipc";
import { copyText } from "../../ui/clipboard";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import {
  availability,
  DANGEROUS_ACTIONS,
  READY_ACTIONS,
  type CommitAction,
} from "./commitActions";

/** What each ready action does when picked (filled in by tasks 10.2–10.5). */
export const ACTION_HANDLERS: Partial<
  Record<CommitAction, (repoId: number, oid: string) => void>
> = {};

/** The actions on one commit of the graph, at (`x`, `y`). Only actions that
 * already work are listed; those that cannot be done on this commit are
 * disabled with the reason as their tooltip. */
export function CommitMenu({
  repoId,
  oid,
  x,
  y,
  onClose,
}: {
  repoId: number;
  oid: string;
  x: number;
  y: number;
  onClose: () => void;
}) {
  const { t } = useTranslation();
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
      onSelect: () => ACTION_HANDLERS[action]?.(repoId, oid),
    };
  };

  const items: MenuItem[] = [
    ...READY_ACTIONS.map(actionItem),
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
