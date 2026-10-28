import { useTranslation } from "react-i18next";
import detail from "../detail/detail.module.css";
import { FileList } from "../detail/FileList";
import { absoluteTime } from "../graph/relativeTime";
import { useStashStore } from "./store";
import styles from "./stash.module.css";

/** Detail panel for a stash entry: what it holds, file by file, and what
 * can be done with it. */
export function StashPanel({ repoId, oid }: { repoId: number; oid: string }) {
  const { t, i18n } = useTranslation();
  const entry = useStashStore(
    (s) => s.stashes[repoId]?.entries?.find((e) => e.oid === oid) ?? null,
  );
  const loaded = useStashStore((s) => s.stashes[repoId]?.entries != null);
  const apply = useStashStore((s) => s.apply);
  const requestDrop = useStashStore((s) => s.requestDrop);
  const busy = useStashStore((s) => s.busy);

  if (!entry) {
    return (
      <p className={detail.muted}>
        {loaded ? t("stash.gone") : t("detail.loading")}
      </p>
    );
  }

  return (
    <article className={detail.commit}>
      {entry.origin !== "user" && (
        <p className={styles.origin}>{t(`stash.originLong.${entry.origin}`)}</p>
      )}
      <h2 className={`${detail.summary} selectable`}>{entry.message}</h2>

      <div className={styles.actions}>
        <button
          type="button"
          className={styles.primary}
          disabled={busy}
          onClick={() =>
            void apply(repoId, entry, { pop: false, saveLocal: false })
          }
        >
          {t("stash.apply")}
        </button>
        <button
          type="button"
          className={styles.secondary}
          disabled={busy}
          onClick={() =>
            void apply(repoId, entry, { pop: true, saveLocal: false })
          }
        >
          {t("stash.popShort")}
        </button>
        <button
          type="button"
          className={styles.danger}
          disabled={busy}
          onClick={() => requestDrop(repoId, entry)}
        >
          {t("stash.dropMenu")}
        </button>
      </div>

      <dl className={detail.facts}>
        <dt>{t("stash.branch")}</dt>
        <dd className="selectable">
          {entry.branch ?? t("sidebar.detachedHead")}
        </dd>
        <dt>{t("stash.date")}</dt>
        <dd>{absoluteTime(entry.time, i18n.language)}</dd>
        <dt>{t("stash.entry")}</dt>
        <dd className="selectable">{`stash@{${entry.index}}`}</dd>
        <dt>{t("detail.commit")}</dt>
        <dd>
          <code className={`${detail.hash} selectable`} title={entry.oid}>
            {entry.oid}
          </code>
        </dd>
      </dl>

      <FileList
        key={entry.oid}
        repoId={repoId}
        oid={entry.oid}
        isMerge={false}
      />
      {entry.untracked && (
        <FileList
          key={entry.untracked}
          repoId={repoId}
          oid={entry.untracked}
          isMerge={false}
          untracked
        />
      )}
    </article>
  );
}
