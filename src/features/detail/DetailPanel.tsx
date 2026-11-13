import { CommitActionButtons } from "../history/CommitActionButtons";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type CommitDetails,
  type Signature,
} from "../../ipc";
import { laneColor } from "../graph/geometry";
import { RefChips } from "../graph/RefChips";
import { labelsByCommit } from "../graph/refLabels";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { selectedStash, useGraphStore, WORKTREE } from "../graph/store";
import { StashPanel } from "../stash/StashPanel";
import { useRememberedScroll } from "../repo/viewState";
import { WorkingDirPanel } from "../workdir/WorkingDirPanel";
import { FileList } from "./FileList";
import { reflowMessage } from "./reflow";
import styles from "./detail.module.css";
import { ErrorReport } from "../../ui/ErrorReport";

type Loaded =
  | { oid: string; details: CommitDetails; error?: undefined }
  | { oid: string; error: CommandError; details?: undefined };

/** Right-hand panel: everything about the selected commit. */
export function DetailPanel({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const graph = useGraphStore((s) =>
    repoId === null ? undefined : s.graphs[repoId],
  );
  const selectedOid = graph?.selectedOid ?? null;
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const panel = useRef<HTMLElement>(null);
  useRememberedScroll(
    panel,
    repoId === null ? null : `${repoId}:detail:${selectedOid ?? ""}`,
  );

  useEffect(() => {
    if (
      repoId === null ||
      selectedOid === null ||
      selectedOid === WORKTREE ||
      selectedStash(selectedOid) !== null
    ) {
      return;
    }
    let stale = false;
    void commands.commitDetails(repoId, selectedOid).then((result) => {
      if (stale) return;
      setLoaded(
        result.status === "ok"
          ? { oid: selectedOid, details: result.data }
          : { oid: selectedOid, error: result.error },
      );
    });
    return () => {
      stale = true;
    };
  }, [repoId, selectedOid]);

  const current = loaded?.oid === selectedOid ? loaded : null;

  return (
    <aside
      ref={panel}
      className={
        selectedOid === WORKTREE && repoId !== null
          ? `${styles.panel} ${styles.panelFill}`
          : styles.panel
      }
      aria-label={t("zones.detail")}
    >
      {selectedOid === null || repoId === null ? (
        <p className={styles.muted}>{t("detail.empty")}</p>
      ) : selectedOid === WORKTREE ? (
        <WorkingDirPanel repoId={repoId} />
      ) : selectedStash(selectedOid) !== null ? (
        <StashPanel repoId={repoId} oid={selectedStash(selectedOid) ?? ""} />
      ) : !current ? (
        <p className={styles.muted}>{t("detail.loading")}</p>
      ) : current.error ? (
        <ErrorReport error={current.error} lead={t("detail.failed")} />
      ) : (
        <CommitInfo repoId={repoId} details={current.details} />
      )}
    </aside>
  );
}

function CommitInfo({
  repoId,
  details,
}: {
  repoId: number;
  details: CommitDetails;
}) {
  const { t } = useTranslation();
  const refs = useGraphStore((s) => s.graphs[repoId]?.refs ?? null);
  const color = useGraphStore(
    (s) =>
      s.graphs[repoId]?.lines.find((l) => l.oid === details.oid)?.color ?? 0,
  );
  const select = useGraphStore((s) => s.select);
  const labels = useMemo(
    () => labelsByCommit(refs).get(details.oid),
    [refs, details.oid],
  );
  const sameCommitter =
    details.committer.name === details.author.name &&
    details.committer.email === details.author.email &&
    details.committer.time === details.author.time;

  return (
    <article className={styles.commit}>
      <RefChips labels={labels} color={laneColor(color)} />
      <h2 className={`${styles.summary} selectable`}>{details.summary}</h2>
      <CommitActionButtons repoId={repoId} oid={details.oid} />
      {details.body && (
        <pre className={`${styles.body} selectable`}>
          {reflowMessage(details.body)}
        </pre>
      )}

      <dl className={styles.facts}>
        <dt>{t("detail.author")}</dt>
        <dd>
          <Person signature={details.author} />
        </dd>
        {!sameCommitter && (
          <>
            <dt>{t("detail.committer")}</dt>
            <dd>
              <Person signature={details.committer} />
            </dd>
          </>
        )}
        <dt>{t("detail.commit")}</dt>
        <dd className={styles.hashRow}>
          <code className={`${styles.hash} selectable`} title={details.oid}>
            {details.oid}
          </code>
          <CopyButton text={details.oid} />
        </dd>
        <dt>{t("detail.parent", { count: details.parents.length })}</dt>
        <dd className={styles.parents}>
          {details.parents.length === 0
            ? t("detail.noParents")
            : details.parents.map((parent) => (
                <button
                  key={parent}
                  type="button"
                  className={styles.parent}
                  title={t("detail.goToParent", { sha: parent })}
                  onClick={() => select(repoId, parent)}
                >
                  <code>{parent.slice(0, 7)}</code>
                </button>
              ))}
        </dd>
      </dl>

      <FileList
        key={details.oid}
        repoId={repoId}
        oid={details.oid}
        isMerge={details.parents.length > 1}
      />
    </article>
  );
}

function Person({ signature }: { signature: Signature }) {
  const { i18n } = useTranslation();
  const language = i18n.language;
  // "Now" for the relative date, fixed when the commit is shown.
  const [now] = useState(() => Date.now());
  return (
    <>
      <span className="selectable">
        {signature.name} &lt;{signature.email}&gt;
      </span>
      <span
        className={styles.date}
        title={absoluteTime(signature.time, language)}
      >
        {relativeTime(signature.time, now, language)} ·{" "}
        {absoluteTime(signature.time, language)}
      </span>
    </>
  );
}

function CopyButton({ text }: { text: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className={styles.copy}
      onClick={() =>
        void navigator.clipboard.writeText(text).then(() => setCopied(true))
      }
      onBlur={() => setCopied(false)}
    >
      {copied ? t("detail.copied") : t("detail.copy")}
    </button>
  );
}
