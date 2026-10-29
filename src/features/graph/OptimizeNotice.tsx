import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, optimizeRepository, type CommandError } from "../../ipc";
import { useGraphStore } from "./store";
import styles from "./graph.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

type Phase =
  | { name: "offer" }
  | { name: "running"; percent: number | null }
  | { name: "done" }
  | { name: "error"; error: CommandError };

/** Last "NN%" in a git progress line, if any. */
export function progressPercent(line: string): number | null {
  const matches = [...line.matchAll(/(\d{1,3})%/g)];
  const last = matches.at(-1)?.[1];
  return last === undefined ? null : Math.min(100, Number(last));
}

/**
 * Discreet offer to write a commit-graph for a large repository. Only runs
 * git when the user clicks; dismissing is remembered for this repository.
 */
export function OptimizeNotice({
  repoId,
  commitCount,
  path,
}: {
  repoId: number;
  commitCount: number;
  path: string;
}) {
  const { t } = useTranslation();
  const hide = useGraphStore((s) => s.hideOptimizeNotice);
  const [phase, setPhase] = useState<Phase>({ name: "offer" });

  const run = async () => {
    setPhase({ name: "running", percent: null });
    const result = await optimizeRepository(repoId, (line) => {
      const percent = progressPercent(line);
      if (percent !== null) setPhase({ name: "running", percent });
    });
    if (result.status === "ok") setPhase({ name: "done" });
    else if (result.error.kind === "cancelled") setPhase({ name: "offer" });
    else setPhase({ name: "error", error: result.error });
  };

  const dismiss = async () => {
    hide(repoId);
    await commands.dismissOptimizeNotice(repoId);
  };

  return (
    <div className={styles.notice} role="status">
      {phase.name === "offer" && (
        <>
          <p>{t("optimize.message", { count: commitCount })}</p>
          <div className={styles.noticeActions}>
            <button
              type="button"
              className={styles.noticePrimary}
              onClick={run}
            >
              {t("optimize.run")}
            </button>
            <button
              type="button"
              className={styles.noticeButton}
              title={t("optimize.dismissHint")}
              onClick={dismiss}
            >
              {t("optimize.dismiss")}
            </button>
          </div>
        </>
      )}
      {phase.name === "running" && (
        <>
          <p>{t("optimize.running")}</p>
          <progress
            className={styles.progress}
            max={100}
            value={phase.percent ?? undefined}
            aria-label={t("optimize.running")}
          />
          <button
            type="button"
            className={styles.noticeButton}
            onClick={() => void commands.cancelOperation(repoId)}
          >
            {t("optimize.cancel")}
          </button>
        </>
      )}
      {phase.name === "done" && (
        <>
          <p>{t("optimize.done")}</p>
          <button
            type="button"
            className={styles.noticeButton}
            onClick={() => hide(repoId)}
          >
            {t("optimize.close")}
          </button>
        </>
      )}
      {phase.name === "error" && (
        <>
          <ErrorReport
            error={phase.error}
            lead={t("optimize.failed")}
            path={path}
          />
          <button
            type="button"
            className={styles.noticeButton}
            onClick={() => setPhase({ name: "offer" })}
          >
            {t("optimize.close")}
          </button>
        </>
      )}
    </div>
  );
}
