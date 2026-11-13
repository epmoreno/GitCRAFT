import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type ConflictSides,
  type ContinueOutcome,
} from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { Tooltip } from "../../ui/Tooltip";
import { refreshAfterRemote, useRemoteStore } from "../remote/store";
import { useWorkdirStore } from "../workdir/store";
import { sideNames } from "./result";
import { unsavedPaths, useConflictStore } from "./store";
import styles from "./banner.module.css";

type Confirm =
  | { kind: "skip" }
  | { kind: "nothingToCommit" }
  | { kind: "unsaved"; paths: string[] };

/** The commit message of the commit box, if the user wrote one. */
function boxMessage(repoId: number): string | null {
  const draft = useWorkdirStore.getState().drafts[repoId];
  if (!draft || draft.summary.trim() === "") return null;
  const description = draft.description.trim();
  return description
    ? `${draft.summary.trim()}\n\n${description}\n`
    : `${draft.summary.trim()}\n`;
}

/**
 * A merge, rebase, cherry-pick or revert waiting for the user: what it is,
 * how far a rebase has got, how many files are left in conflict, and
 * Continue / Skip this commit / Abort.
 */
export function InProgressBanner({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const status = useWorkdirStore((s) =>
    repoId === null ? null : (s.statuses[repoId]?.status ?? null),
  );
  const requestAbort = useRemoteStore((s) => s.requestAbort);
  const operation = status?.inProgress ?? null;
  const conflicts = status?.conflicted.length ?? 0;
  const [sides, setSides] = useState<ConflictSides | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  // Re-read the sides as the operation moves on (a rebase's next commit).
  const key = JSON.stringify([
    repoId,
    operation,
    status?.branch.oid,
    conflicts,
  ]);

  useEffect(() => {
    if (repoId === null || !operation) return;
    let stale = false;
    void commands.conflictSides(repoId).then((result) => {
      if (!stale) setSides(result.status === "ok" ? result.data : null);
    });
    return () => {
      stale = true;
    };
  }, [repoId, operation, key]);

  if (repoId === null || !operation) return null;
  const id = repoId;
  const current = sides?.type === operation ? sides : null;
  const names = sideNames(current, (short) =>
    t("conflicts.revertOf", { short }),
  );
  const canSkip = operation !== "merge";

  async function handle(
    result:
      | { status: "ok"; data: ContinueOutcome }
      | { status: "error"; error: CommandError },
  ) {
    setBusy(false);
    refreshAfterRemote(id);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    switch (result.data.type) {
      case "done":
        if (operation === "merge") {
          useWorkdirStore
            .getState()
            .setDraft(id, { summary: "", description: "" });
        }
        return;
      case "nothingToCommit":
        setConfirm({ kind: "nothingToCommit" });
        return;
      case "stoppedAgain":
      case "conflictsLeft":
        return;
    }
  }

  async function continueNow(force = false) {
    const unsaved = unsavedPaths(id);
    if (!force && unsaved.length > 0) {
      setConfirm({ kind: "unsaved", paths: unsaved });
      return;
    }
    useConflictStore.getState().clearRepo(id);
    setBusy(true);
    setError(null);
    await handle(
      await commands.continueInProgress(
        id,
        operation === "merge" ? boxMessage(id) : null,
      ),
    );
  }

  async function skipNow() {
    useConflictStore.getState().clearRepo(id);
    setBusy(true);
    setError(null);
    await handle(await commands.skipCurrent(id));
  }

  async function confirmed() {
    const pending = confirm;
    setConfirm(null);
    if (pending?.kind === "unsaved") await continueNow(true);
    else await skipNow();
  }

  const applying =
    current?.type === "rebase"
      ? current.applying
      : current?.type === "cherryPick"
        ? current.picking
        : current?.type === "revert"
          ? current.reverting
          : null;

  const title = !current
    ? t(`pull.inProgress.${operation}`)
    : current.type === "merge"
      ? t("operation.title.merge", { ours: names.ours, theirs: names.theirs })
      : current.type === "rebase"
        ? t("operation.title.rebase", {
            branch: names.theirs,
            onto: names.ours,
          })
        : t(`operation.title.${current.type}`, {
            ours: names.ours,
            short: applying?.short ?? "",
            subject: applying?.subject ?? "",
          });
  const progress =
    current?.type === "rebase" && current.progress
      ? t("operation.progress", {
          current: current.progress.current,
          total: current.progress.total,
          subject: current.applying?.subject ?? "",
        })
      : null;

  const continueButton = (
    <button
      type="button"
      className={styles.primary}
      disabled={busy || conflicts > 0}
      onClick={() => void continueNow()}
    >
      {t("operation.continue")}
    </button>
  );

  return (
    <div className={styles.banner} role="status">
      <div className={styles.text}>
        <p className={styles.title}>
          {title}
          {progress && <span className={styles.progress}> · {progress}</span>}
        </p>
        <p>
          {conflicts > 0
            ? t("operation.conflicts", { count: conflicts })
            : t(`operation.ready.${operation}`)}
        </p>
        {error && <ErrorReport error={error} lead={t("operation.failed")} />}
      </div>
      {canSkip && (
        <button
          type="button"
          className={styles.secondary}
          disabled={busy}
          onClick={() => setConfirm({ kind: "skip" })}
        >
          {t("operation.skip")}
        </button>
      )}
      <button
        type="button"
        className={styles.danger}
        disabled={busy}
        onClick={() => requestAbort(id, operation)}
      >
        {t("pull.abortButton")}
      </button>
      {conflicts > 0 ? (
        <Tooltip label={t("operation.resolveFirst")}>{continueButton}</Tooltip>
      ) : (
        continueButton
      )}
      {confirm && (
        <ConfirmDialog
          title={t(`operation.confirm.${confirm.kind}.title`)}
          confirmLabel={t(`operation.confirm.${confirm.kind}.confirm`)}
          cancelLabel={t("conflicts.cancel")}
          danger
          onConfirm={() => void confirmed()}
          onCancel={() => setConfirm(null)}
        >
          <p>
            {t(`operation.confirm.${confirm.kind}.body`, {
              short: applying?.short ?? "",
              subject: applying?.subject ?? "",
            })}
          </p>
          {confirm.kind === "unsaved" && (
            <ul>
              {confirm.paths.map((path) => (
                <li key={path}>
                  <code>{path}</code>
                </li>
              ))}
            </ul>
          )}
        </ConfirmDialog>
      )}
    </div>
  );
}
