import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type CommitActions } from "../../ipc";
import { Tooltip } from "../../ui/Tooltip";
import {
  availability,
  DANGEROUS_ACTIONS,
  READY_ACTIONS,
} from "../graph/commitActions";
import { useWorkdirStore } from "../workdir/store";
import { runCommitAction } from "./store";
import styles from "./history.module.css";

/** The actions of the commit menu, as buttons in the detail panel. Only
 * those that already work; a disabled one says why in its tooltip. */
export function CommitActionButtons({
  repoId,
  oid,
}: {
  repoId: number;
  oid: string;
}) {
  const { t } = useTranslation();
  const [facts, setFacts] = useState<CommitActions | null>(null);
  // Re-read when the status changes (an operation started or ended).
  const version = useWorkdirStore((s) => s.statuses[repoId]?.version ?? 0);
  const key = JSON.stringify([repoId, oid, version]);
  useEffect(() => {
    if (READY_ACTIONS.length === 0) return;
    let stale = false;
    const [id, commit] = JSON.parse(key) as [number, string];
    void commands.commitActions(id, commit).then((result) => {
      if (!stale && result.status === "ok") setFacts(result.data);
    });
    return () => {
      stale = true;
    };
  }, [key]);

  if (READY_ACTIONS.length === 0 || !facts || facts.oid !== oid) return null;
  const branch = facts.branch ?? "";
  return (
    <div
      className={styles.actions}
      role="group"
      aria-label={t("history.actions")}
    >
      {READY_ACTIONS.map((action) => {
        const state = availability(action, facts);
        const danger = DANGEROUS_ACTIONS.has(action);
        const button = (
          <button
            key={action}
            type="button"
            className={danger ? styles.dangerAction : styles.action}
            disabled={!state.enabled}
            onClick={() => runCommitAction(action, repoId, oid)}
          >
            {t(`commitMenu.button.${action}`)}
          </button>
        );
        return [
          danger && <span key={`${action}-sep`} className={styles.separator} />,
          state.enabled ? (
            button
          ) : (
            <Tooltip
              key={action}
              label={t(`commitMenu.why.${state.reason}`, { branch })}
            >
              {button}
            </Tooltip>
          ),
        ];
      })}
    </div>
  );
}
