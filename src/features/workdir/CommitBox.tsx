import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type AmendInfo,
  type CommandError,
  type WorkingStatus,
} from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { useWorkdirStore } from "./store";
import styles from "./workdir.module.css";

/** Summaries longer than this are hard to read in one-line logs. */
export const SUMMARY_LIMIT = 72;

/** Summary + description, amend, and the commit button. */
export function CommitBox({
  repoId,
  status,
}: {
  repoId: number;
  status: WorkingStatus;
}) {
  const { t } = useTranslation();
  const draft = useWorkdirStore((s) => s.drafts[repoId]) ?? {
    summary: "",
    description: "",
    amend: false,
  };
  const busy = useWorkdirStore((s) => s.statuses[repoId]?.busy ?? false);
  const setDraft = useWorkdirStore((s) => s.setDraft);
  const commit = useWorkdirStore((s) => s.commit);
  const [amendInfo, setAmendInfo] = useState<AmendInfo | null>(null);
  const [confirming, setConfirming] = useState(false);
  /** Why amend could not be turned on. */
  const [amendProblem, setAmendProblem] = useState<
    CommandError | "nothing" | null
  >(null);

  const staged = status.staged.length;
  const blocked =
    status.conflicted.length > 0
      ? t("commit.blockedConflicts")
      : staged === 0 && !draft.amend
        ? t("commit.blockedNothingStaged")
        : draft.summary.trim() === ""
          ? t("commit.blockedNoSummary")
          : null;

  const toggleAmend = async (amend: boolean) => {
    setDraft(repoId, { amend });
    setAmendProblem(null);
    if (!amend) return;
    const result = await commands.amendInfo(repoId);
    if (result.status !== "ok" || !result.data) {
      setDraft(repoId, { amend: false });
      setAmendProblem(result.status === "ok" ? "nothing" : result.error);
      return;
    }
    setAmendInfo(result.data);
    // Prefill with the last message unless the user already wrote one.
    if (!draft.summary.trim() && !draft.description.trim()) {
      setDraft(repoId, {
        summary: result.data.summary,
        description: result.data.body,
      });
    }
  };

  const submit = () => {
    if (blocked || busy) return;
    if (draft.amend && amendInfo && amendInfo.publishedIn.length > 0) {
      setConfirming(true);
    } else {
      void commit(repoId);
    }
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      submit();
    }
  };

  const length = [...draft.summary].length;
  const tooLong = length > SUMMARY_LIMIT;

  return (
    <form
      className={styles.commitBox}
      aria-label={t("commit.label")}
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <div className={styles.summaryRow}>
        <input
          className={styles.summary}
          value={draft.summary}
          placeholder={t("commit.summaryPlaceholder")}
          aria-label={t("commit.summary")}
          onChange={(e) => setDraft(repoId, { summary: e.target.value })}
          onKeyDown={onKeyDown}
        />
        <span
          className={
            tooLong ? `${styles.counter} ${styles.counterWarn}` : styles.counter
          }
          title={
            tooLong
              ? t("commit.summaryTooLong", { limit: SUMMARY_LIMIT })
              : undefined
          }
          aria-label={t("commit.counter", {
            count: length,
            limit: SUMMARY_LIMIT,
          })}
        >
          {length}/{SUMMARY_LIMIT}
        </span>
      </div>
      <textarea
        className={styles.description}
        value={draft.description}
        rows={4}
        placeholder={t("commit.descriptionPlaceholder")}
        aria-label={t("commit.description")}
        onChange={(e) => setDraft(repoId, { description: e.target.value })}
        onKeyDown={onKeyDown}
      />
      <label className={styles.amend}>
        <input
          type="checkbox"
          checked={draft.amend}
          onChange={(e) => void toggleAmend(e.target.checked)}
        />
        {t("commit.amend")}
      </label>
      {amendProblem === "nothing" ? (
        <p className={styles.hint}>{t("commit.nothingToAmend")}</p>
      ) : (
        amendProblem && (
          <ErrorReport error={amendProblem} lead={t("commit.amendFailed")} />
        )
      )}
      {blocked && <p className={styles.hint}>{blocked}</p>}
      <button
        type="submit"
        className={styles.commitButton}
        disabled={blocked !== null || busy}
        title={t("commit.shortcut")}
      >
        {draft.amend
          ? t("commit.amendButton")
          : t("commit.button", { count: staged })}
      </button>

      {confirming && amendInfo && (
        <ConfirmDialog
          title={t("commit.publishedTitle")}
          confirmLabel={t("commit.publishedConfirm")}
          cancelLabel={t("commit.cancel")}
          danger
          onCancel={() => setConfirming(false)}
          onConfirm={() => {
            setConfirming(false);
            void commit(repoId);
          }}
        >
          <p>
            {t("commit.publishedWhere", {
              summary: amendInfo.summary,
              sha: amendInfo.oid.slice(0, 7),
              branches: amendInfo.publishedIn.join(", "),
            })}
          </p>
          <p>{t("commit.publishedConsequence")}</p>
          <p>{t("commit.publishedRecovery")}</p>
        </ConfirmDialog>
      )}
    </form>
  );
}
