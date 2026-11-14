import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { MovePreview } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { Files } from "../history/HistoryDialogs";
import historyStyles from "../history/history.module.css";
import { BranchSketch } from "./BranchSketch";
import { useMergeStore, type MergeDialogState } from "./store";
import styles from "./merge.module.css";

/** At most this many commits are listed (the user's choice). */
export const MAX_COMMITS = 20;

/** The commits that come in or are replayed, at most [`MAX_COMMITS`], and
 * "and N more". A rebase numbers them (the order they are replayed in) and
 * marks the published ones. */
function Commits({
  preview,
  heading,
  rebase = false,
}: {
  preview: MovePreview;
  heading: string;
  rebase?: boolean;
}) {
  const { t } = useTranslation();
  const shown = preview.commits.slice(0, MAX_COMMITS);
  const more = preview.commitsTotal - shown.length;
  const List = rebase ? "ol" : "ul";
  return (
    <>
      <h3 className={historyStyles.heading}>{heading}</h3>
      <List className={historyStyles.files}>
        {shown.map(({ commit, published }) => (
          <li key={commit.oid}>
            <code>{commit.short}</code> {commit.subject}
            {rebase && published && (
              <span className={historyStyles.note}>
                {" "}
                {t("rebase.published")}
              </span>
            )}
          </li>
        ))}
        {more > 0 && (
          <li className={historyStyles.note}>
            {t("history.more", { count: more })}
          </li>
        )}
      </List>
    </>
  );
}

/** When the source's remote was last fetched, and "Fetch first". */
function FetchFirst({ dialog }: { dialog: MergeDialogState }) {
  const { t, i18n } = useTranslation();
  const fetchFirst = useMergeStore((s) => s.fetchFirst);
  // The time the dialog opened is "now" for "3 hours ago".
  const [now] = useState(() => Date.now());
  const remote = dialog.remote;
  if (!remote) return null;
  const at = dialog.lastFetch?.at ?? null;
  // A fetch made after the dialog opened is "now", not in the future.
  const when =
    at === null
      ? null
      : relativeTime(
          Math.min(at, now / 1000),
          now,
          i18n.resolvedLanguage ?? "es",
        );
  return (
    <div className={styles.fetchRow}>
      <span
        title={
          at === null
            ? undefined
            : absoluteTime(at, i18n.resolvedLanguage ?? "es")
        }
      >
        {when === null
          ? t("merge.neverFetched", { remote })
          : t(
              dialog.lastFetch?.exact ? "merge.lastFetch" : "merge.lastChange",
              {
                remote,
                when,
              },
            )}
      </span>
      <button
        type="button"
        className={styles.fetchButton}
        disabled={dialog.fetching}
        onClick={() => void fetchFirst()}
      >
        {dialog.fetching ? t("merge.fetching") : t("merge.fetchFirst")}
      </button>
    </div>
  );
}

function Body({
  dialog,
  preview,
  noFf,
  setNoFf,
  message,
  setMessage,
}: {
  dialog: MergeDialogState;
  preview: MovePreview;
  noFf: boolean;
  setNoFf: (value: boolean) => void;
  message: string;
  setMessage: (value: string) => void;
}) {
  const { t } = useTranslation();
  const values = {
    source: preview.source.name,
    target: preview.target.name,
    count: preview.commitsTotal,
  };
  const isCurrent = preview.current === preview.target.name;
  const fastForward = preview.case === "fastForward";
  const commit = preview.case === "merge" || (fastForward && noFf);
  const refused = preview.refused;
  return (
    <>
      {dialog.changed && (
        <p className={historyStyles.warning} role="alert">
          {t("merge.changed")}
        </p>
      )}
      <FetchFirst dialog={dialog} />
      {refused ? (
        <p className={historyStyles.warning}>
          {t(`merge.refused.${refused}`, {
            ...values,
            moving: preview.moving,
            current: preview.current ?? "",
          })}
        </p>
      ) : preview.case === "upToDate" ? (
        <p>{t("merge.upToDate", values)}</p>
      ) : (
        <>
          <BranchSketch
            kind={commit ? "merge" : "fastForward"}
            moving={preview.target.name}
            other={preview.source.name}
          />
          <p>
            {commit
              ? t("merge.explained.merge", values)
              : isCurrent
                ? t("merge.explained.fastForward", values)
                : t("merge.explained.fastForwardOther", values)}
          </p>
          <Commits preview={preview} heading={t("merge.incoming", values)} />
          {fastForward && isCurrent && (
            <label className={styles.option}>
              <input
                type="checkbox"
                checked={noFf}
                onChange={(e) => setNoFf(e.target.checked)}
              />
              {t("merge.noFf")}
            </label>
          )}
          {commit && (
            <label className={historyStyles.field}>
              {t("merge.message")}
              <textarea
                className={`${historyStyles.input} ${styles.message}`}
                value={message}
                spellCheck={false}
                onChange={(e) => setMessage(e.target.value)}
              />
            </label>
          )}
          {preview.blocked && (
            <section aria-label={t("history.blockedTitle")}>
              <h3 className={historyStyles.heading}>
                {t("history.blockedTitle")}
              </h3>
              <p>{t(`merge.blocked.${preview.blocked}`)}</p>
              <Files files={preview.blocking} />
              <p>{t("merge.saveExplained", values)}</p>
            </section>
          )}
          {commit && preview.conflicts.length > 0 && (
            <section aria-label={t("history.conflictsTitle")}>
              <h3 className={historyStyles.heading}>
                {t(preview.exact ? "merge.conflicts" : "merge.mayConflict", {
                  count: preview.conflicts.length,
                })}
              </h3>
              <Files files={preview.conflicts.map((path) => ({ path }))} />
              <p>{t("history.conflictExplained")}</p>
              {preview.blocked && <p>{t("history.abortKeepsSavedHere")}</p>}
            </section>
          )}
        </>
      )}
    </>
  );
}

/** The rebase body (task 8.3). */
function RebaseBody({
  dialog,
  preview,
}: {
  dialog: MergeDialogState;
  preview: MovePreview;
}) {
  const { t } = useTranslation();
  const values = {
    source: preview.source.name,
    target: preview.target.name,
    count: preview.commitsTotal,
  };
  const isCurrent = preview.current === preview.source.name;
  const fastForward = preview.case === "fastForward";
  const published = preview.commits.filter((c) => c.published).length;
  const refused = preview.refused;
  const stop = preview.stopsAt;
  return (
    <>
      {dialog.changed && (
        <p className={historyStyles.warning} role="alert">
          {t("merge.changed")}
        </p>
      )}
      <FetchFirst dialog={dialog} />
      {refused ? (
        <p className={historyStyles.warning}>
          {t(`rebase.refused.${refused}`, {
            ...values,
            moving: preview.moving,
            current: preview.current ?? "",
          })}
        </p>
      ) : preview.case === "upToDate" ? (
        <p>{t("rebase.upToDate", values)}</p>
      ) : (
        <>
          <BranchSketch
            kind={fastForward ? "fastForward" : "rebase"}
            moving={preview.source.name}
            other={preview.target.name}
          />
          <p>
            {fastForward
              ? t(
                  isCurrent
                    ? "rebase.explained.fastForward"
                    : "rebase.explained.fastForwardOther",
                  values,
                )
              : t("rebase.explained.rebase", values)}
          </p>
          <Commits
            preview={preview}
            heading={t(
              fastForward ? "rebase.gained" : "rebase.replayed",
              values,
            )}
            rebase={!fastForward}
          />
          {preview.skipped > 0 && (
            <p className={historyStyles.note}>
              {t("rebase.skipped", { ...values, count: preview.skipped })}
            </p>
          )}
          {published > 0 && (
            <p className={historyStyles.warning}>
              {t("rebase.publishedWarning", {
                ...values,
                count: published,
                remotes: preview.publishedIn.join(", "),
              })}
            </p>
          )}
          {preview.blocked && (
            <section aria-label={t("history.blockedTitle")}>
              <h3 className={historyStyles.heading}>
                {t("history.blockedTitle")}
              </h3>
              <p>{t(`rebase.blocked.${preview.blocked}`)}</p>
              <Files files={preview.blocking} />
              <p>{t("rebase.saveExplained", values)}</p>
            </section>
          )}
          {preview.conflicts.length > 0 && (
            <section aria-label={t("history.conflictsTitle")}>
              <h3 className={historyStyles.heading}>
                {stop
                  ? t("rebase.stopsAt", {
                      index: stop.index,
                      total: preview.commitsTotal,
                      subject: stop.commit.subject,
                    })
                  : t("rebase.mayConflict", {
                      count: preview.conflicts.length,
                    })}
              </h3>
              <Files files={preview.conflicts.map((path) => ({ path }))} />
              {stop && <p>{t("rebase.afterStop")}</p>}
              <p>{t("history.conflictExplained")}</p>
              {preview.blocked && <p>{t("history.abortKeepsSavedHere")}</p>}
            </section>
          )}
        </>
      )}
    </>
  );
}

/** The merge (task 8.2) or rebase (8.3) confirmation. */
export function MergeDialog() {
  const dialog = useMergeStore((s) => s.dialog);
  if (!dialog) return null;
  return <MergeDialogOpen key={`${dialog.source}>${dialog.target}`} />;
}

function MergeDialogOpen() {
  const { t } = useTranslation();
  const dialog = useMergeStore((s) => s.dialog);
  const busy = useMergeStore((s) => s.busy);
  const close = useMergeStore((s) => s.close);
  const confirm = useMergeStore((s) => s.confirm);
  const [noFf, setNoFf] = useState(false);
  // `null` until the user edits it: git's message, also after a new preview.
  const [edited, setEdited] = useState<string | null>(null);
  if (!dialog) return null;
  const preview = dialog.preview;
  const op = dialog.kind;
  const source = preview?.source.name ?? shortName(dialog.source);
  const target = preview?.target.name ?? shortName(dialog.target);
  const message = edited ?? preview?.message ?? "";
  const fastForward = preview?.case === "fastForward";
  const commit =
    op === "merge" && (preview?.case === "merge" || (fastForward && noFf));
  const conflicts =
    (op === "rebase" || commit) && (preview?.conflicts.length ?? 0) > 0;
  const blocked = !!preview?.blocked;
  const action = fastForward && !(op === "merge" && noFf) ? "advance" : "act";
  const label = blocked
    ? conflicts
      ? "saveAndActWithConflicts"
      : action === "advance"
        ? "saveAndAdvance"
        : "saveAndAct"
    : conflicts
      ? "actWithConflicts"
      : action;
  const doable =
    !!preview &&
    !preview.refused &&
    preview.case !== "upToDate" &&
    !(commit && message.trim() === "");
  return (
    <ConfirmDialog
      title={t(`${op}.title`, { source, target })}
      confirmLabel={t(`${op}.${label}`, { source, target })}
      cancelLabel={t("history.cancel")}
      // A rebase that rewrites published commits: the danger style.
      danger={op === "rebase" && !!preview?.commits.some((c) => c.published)}
      confirmDisabled={!doable || busy || dialog.fetching}
      onConfirm={() =>
        void confirm({
          noFf: fastForward && noFf,
          message: commit ? message : null,
        })
      }
      onCancel={close}
    >
      {!preview ? (
        <p className={historyStyles.note}>{t("merge.loading")}</p>
      ) : op === "rebase" ? (
        <RebaseBody dialog={dialog} preview={preview} />
      ) : (
        <Body
          dialog={dialog}
          preview={preview}
          noFf={noFf}
          setNoFf={setNoFf}
          message={message}
          setMessage={setEdited}
        />
      )}
    </ConfirmDialog>
  );
}

function shortName(refname: string) {
  return refname.replace(/^refs\/(heads|remotes)\//, "");
}
