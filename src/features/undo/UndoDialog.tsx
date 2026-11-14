import { useTranslation } from "react-i18next";
import type { BranchChange, CommitRef, UndoPreview } from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { Files } from "../history/HistoryDialogs";
import styles from "../history/history.module.css";
import { describeOp } from "./describe";
import { useUndoStore } from "./store";

/** At most this many commits are listed per branch. */
const MAX_COMMITS = 20;

const short = (oid: string | null) => (oid ?? "").slice(0, 7);

function Commits({ commits, total }: { commits: CommitRef[]; total: number }) {
  const { t } = useTranslation();
  const shown = commits.slice(0, MAX_COMMITS);
  return (
    <ul className={styles.files}>
      {shown.map((c) => (
        <li key={c.oid}>
          <code>{c.short}</code> «{c.subject}»
        </li>
      ))}
      {total > shown.length && (
        <li className={styles.note}>
          {t("history.more", { count: total - shown.length })}
        </li>
      )}
    </ul>
  );
}

/** What happens to one branch: it moves, comes back or is deleted, with
 * the commits that leave it and the ones that come in. */
function Branch({ change }: { change: BranchChange }) {
  const { t } = useTranslation();
  const { branch } = change;
  const line = !change.from
    ? t("undo.branch.created", { branch, short: short(change.to) })
    : !change.to
      ? t("undo.branch.deleted", { branch })
      : t("undo.branch.moves", { branch, short: short(change.to) });
  return (
    <section>
      <p>{line}</p>
      {change.leavingTotal > 0 && (
        <>
          <h3 className={styles.heading}>
            {t("undo.branch.leaving", { branch, count: change.leavingTotal })}
          </h3>
          <Commits commits={change.leaving} total={change.leavingTotal} />
          {change.onlyInReflog > 0 && (
            <p>
              {t("undo.branch.onlyInReflog", { count: change.onlyInReflog })}
            </p>
          )}
        </>
      )}
      {!change.to && change.leavingTotal === 0 && (
        <p>{t("undo.branch.keptElsewhere")}</p>
      )}
      {change.arrivingTotal > 0 && (
        <>
          <h3 className={styles.heading}>
            {t("undo.branch.arriving", {
              branch,
              count: change.arrivingTotal,
            })}
          </h3>
          <Commits commits={change.arriving} total={change.arrivingTotal} />
        </>
      )}
    </section>
  );
}

/** Why it cannot be done. */
function Refusal({ preview }: { preview: UndoPreview }) {
  const { t } = useTranslation();
  const refused = preview.refused;
  if (!refused) return null;
  const action = t(`undo.verb.${preview.direction}`);
  let text: string;
  switch (refused.type) {
    case "inProgress":
      text = t("undo.refused.inProgress");
      break;
    case "changed":
      text = t(`undo.refused.changed.${preview.direction}`);
      break;
    case "checkedOutElsewhere":
      text = t("undo.refused.checkedOutElsewhere", { branch: refused.branch });
      break;
    case "published": {
      const [first] = refused.commits;
      text =
        refused.commits.length === 1 && first
          ? t("undo.refused.publishedOne", {
              action,
              short: first.short,
              subject: first.subject,
              branch: refused.branch,
              remotes: refused.remotes.join(", "),
            })
          : t("undo.refused.publishedMany", {
              action,
              count: refused.commits.length,
              branch: refused.branch,
              remotes: refused.remotes.join(", "),
            });
      break;
    }
  }
  return (
    <p className={styles.warning} role="alert">
      {text}
    </p>
  );
}

/** What will be restored (or redone), from the preview. */
function Body({ preview }: { preview: UndoPreview }) {
  const { t } = useTranslation();
  const { direction, kind } = preview;
  const switches =
    kind.type !== "renameBranch" &&
    JSON.stringify(preview.headFrom) !== JSON.stringify(preview.headTo);
  const committed = kind.type === "commit" || kind.type === "amend";
  return (
    <>
      {preview.branches.map((change) => (
        <Branch key={change.branch} change={change} />
      ))}
      {preview.tags.map((tag) => {
        const name = tag.refname.replace(/^refs\/tags\//, "");
        return (
          <p key={tag.refname}>
            {!tag.before
              ? t("undo.tag.created", { name, short: short(tag.after) })
              : !tag.after
                ? t("undo.tag.deleted", { name })
                : t("undo.tag.moves", { name, short: short(tag.after) })}
          </p>
        );
      })}
      {switches &&
        (preview.headTo.type === "branch" ? (
          <p>
            {t("undo.head.branch", {
              branch: preview.headTo.name.replace(/^refs\/heads\//, ""),
            })}
          </p>
        ) : (
          <p>{t("undo.head.detached", { short: short(preview.headTo.oid) })}</p>
        ))}
      {direction === "undo" && committed && <p>{t("undo.staged")}</p>}
      {preview.exact ? (
        <>
          <p>{t("undo.exact")}</p>
          {preview.restoresStash && <p>{t("undo.restoresStash")}</p>}
          {preview.restoresTrashed.length > 0 && (
            <>
              <h3 className={styles.heading}>{t("undo.restoresTrashed")}</h3>
              <Files
                files={preview.restoresTrashed.map((path) => ({ path }))}
              />
            </>
          )}
        </>
      ) : (
        <>
          {preview.keepsChanges ? (
            <p>{t("undo.keepsChanges")}</p>
          ) : (
            <p>{t("undo.kept")}</p>
          )}
          {preview.keptStash && (
            <p>{t("undo.keptStash", { short: short(preview.keptStash) })}</p>
          )}
        </>
      )}
      {preview.blocking.length > 0 && (
        <section aria-label={t("undo.blockedTitle")}>
          <h3 className={styles.heading}>{t("undo.blockedTitle")}</h3>
          <Files files={preview.blocking} />
          <p>{t(`undo.saveExplained.${direction}`)}</p>
        </section>
      )}
    </>
  );
}

export function UndoDialog() {
  const { t } = useTranslation();
  const dialog = useUndoStore((s) => s.dialog);
  const busy = useUndoStore((s) => s.busy);
  const close = useUndoStore((s) => s.close);
  const confirm = useUndoStore((s) => s.confirm);
  const revertInstead = useUndoStore((s) => s.revertInstead);
  if (!dialog) return null;
  const { preview, direction } = dialog;
  const what = describeOp(t, preview.kind);
  const refused = preview.refused;
  // One published commit in the way: revert it instead (task 10).
  const revertable =
    refused?.type === "published" && refused.commits.length === 1
      ? refused.commits[0]
      : undefined;
  const blocked = preview.blocking.length > 0;
  const confirmLabel = revertable
    ? t("undo.revert", { subject: revertable.subject })
    : t(blocked ? `undo.saveAndAct.${direction}` : `undo.act.${direction}`);
  return (
    <ConfirmDialog
      title={t(`undo.title.${direction}`, { what })}
      confirmLabel={confirmLabel}
      cancelLabel={t(refused ? "undo.close" : "history.cancel")}
      confirmDisabled={busy || (!!refused && !revertable)}
      onConfirm={() => {
        if (revertable) revertInstead(revertable.oid);
        else void confirm(blocked);
      }}
      onCancel={close}
    >
      {dialog.changed && (
        <p className={styles.warning} role="alert">
          {t("undo.changedPreview")}
        </p>
      )}
      <Refusal preview={preview} />
      {!refused && <Body preview={preview} />}
    </ConfirmDialog>
  );
}
