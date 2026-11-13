import { useConflictStore } from "../conflicts/store";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import branches from "../branches/branches.module.css";
import { useRemoteStore } from "./store";

const MAX_LISTED = 12;

function Files({ files }: { files: { path: string; note?: string }[] }) {
  const { t } = useTranslation();
  return (
    <ul className={branches.fileList}>
      {files.slice(0, MAX_LISTED).map(({ path, note }) => (
        <li key={path} className={branches.file}>
          <span>{path}</span>
          {note && <span className={branches.author}>{note}</span>}
        </li>
      ))}
      {files.length > MAX_LISTED && (
        <li className={branches.more}>
          {t("stash.moreFiles", { count: files.length - MAX_LISTED })}
        </li>
      )}
    </ul>
  );
}

/** Whichever remote dialog is open. */
export function RemoteDialogs() {
  const dialog = useRemoteStore((s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "pullBlocked":
      return <PullBlockedDialog />;
    case "pullConflict":
      return <PullConflictDialog />;
    case "abort":
      return <AbortDialog />;
    case "publish":
      return <PublishDialog />;
    case "pushRejected":
      return <PushRejectedDialog />;
    case "forcePush":
      return <ForcePushDialog />;
  }
}

/** Uncommitted changes stop the pull: nothing was touched. */
function PullBlockedDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const saveLocalAndPull = useRemoteStore((s) => s.saveLocalAndPull);
  if (dialog?.kind !== "pullBlocked") return null;
  const files = dialog.files.map((f) => ({
    path: f.path,
    note: f.untracked ? t("branches.untracked") : undefined,
  }));

  if (dialog.reason === "unresolved") {
    return (
      <ConfirmDialog
        title={t("pull.blockedTitle")}
        confirmLabel={t("pull.ok")}
        cancelLabel={t("pull.cancel")}
        onCancel={close}
        onConfirm={close}
      >
        <p>{t("pull.unresolved")}</p>
        {files.length > 0 && <Files files={files} />}
      </ConfirmDialog>
    );
  }
  return (
    <ConfirmDialog
      title={t("pull.blockedTitle")}
      confirmLabel={t("pull.saveLocalAndPull")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={() => void saveLocalAndPull()}
    >
      <p>{t(`pull.blocked.${dialog.reason}`, { count: files.length })}</p>
      <Files files={files} />
      <p>{t("pull.notForced")}</p>
      <p>{t(`pull.alternatives.${dialog.reason}`)}</p>
    </ConfirmDialog>
  );
}

/** The pull would conflict: nothing was touched. */
function PullConflictDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const pullWithConflicts = useRemoteStore((s) => s.pullWithConflicts);
  const mode = useRemoteStore((s) =>
    dialog ? (s.pullModes[dialog.repoId]?.mode ?? "merge") : "merge",
  );
  if (dialog?.kind !== "pullConflict") return null;
  return (
    <ConfirmDialog
      title={t("pull.conflictTitle")}
      confirmLabel={t("pull.pullWithConflicts")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={() => void pullWithConflicts()}
    >
      <p>
        {t(dialog.exact ? "pull.conflicts" : "pull.mayConflict", {
          count: dialog.files.length,
          upstream: dialog.upstream,
        })}
      </p>
      <Files files={dialog.files.map((path) => ({ path }))} />
      <p>{t("pull.notForced")}</p>
      <p>{t(`pull.conflictExplained.${mode}`)}</p>
    </ConfirmDialog>
  );
}

/** Aborting the merge/rebase a pull left waiting. */
export function AbortDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const confirm = useRemoteStore((s) => s.confirmAbort);
  const drafts = useConflictStore((s) =>
    dialog?.kind === "abort" ? s.drafts[dialog.repoId] : undefined,
  );
  if (dialog?.kind !== "abort") return null;
  const unsaved = Object.keys(drafts ?? {}).sort();
  return (
    <ConfirmDialog
      title={t(`pull.abortTitle.${dialog.operation}`)}
      confirmLabel={t("pull.abort")}
      cancelLabel={t("pull.cancel")}
      danger
      onCancel={close}
      onConfirm={() => {
        useConflictStore.getState().clearRepo(dialog.repoId);
        void confirm();
      }}
    >
      <p>{t("pull.abortExplained")}</p>
      {unsaved.length > 0 && (
        <>
          <p>{t("conflicts.unsaved.abort")}</p>
          <ul>
            {unsaved.map((path) => (
              <li key={path}>
                <code>{path}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </ConfirmDialog>
  );
}

/** The branch has no upstream: publish it (creating one). */
function PublishDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const push = useRemoteStore((s) => s.push);
  const remotes = dialog?.kind === "publish" ? dialog.remotes : [];
  const [remote, setRemote] = useState(
    remotes.includes("origin") ? "origin" : (remotes[0] ?? ""),
  );
  if (dialog?.kind !== "publish") return null;
  return (
    <ConfirmDialog
      title={t("push.publishTitle", { branch: dialog.branch })}
      confirmLabel={t("push.publish")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={() => void push(dialog.repoId, { publishTo: remote })}
    >
      <p>{t("push.noUpstream", { branch: dialog.branch })}</p>
      {remotes.length > 1 && (
        <label className={branches.field}>
          <span>{t("push.remote")}</span>
          <select
            className={branches.input}
            value={remote}
            onChange={(event) => setRemote(event.target.value)}
          >
            {remotes.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
      )}
      <p>{t("push.publishExplained", { branch: dialog.branch, remote })}</p>
    </ConfirmDialog>
  );
}

/** The remote refused the push; nothing changed there. */
function PushRejectedDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const pullAfterRejection = useRemoteStore((s) => s.pullAfterRejection);
  if (dialog?.kind !== "pushRejected") return null;
  const remoteAhead = dialog.reason === "remoteAhead";
  return (
    <ConfirmDialog
      title={t(`push.rejectedTitle.${dialog.reason}`)}
      confirmLabel={remoteAhead ? t("push.pullFirst") : t("pull.ok")}
      cancelLabel={t("pull.cancel")}
      onCancel={close}
      onConfirm={remoteAhead ? () => void pullAfterRejection() : close}
    >
      <p>
        {t(`push.rejected.${dialog.reason}`, { upstream: dialog.upstream })}
      </p>
      <p>{t("push.nothingChanged")}</p>
      {remoteAhead && <p>{t("push.forceHint")}</p>}
      <pre className={`${branches.command} selectable`}>{dialog.detail}</pre>
    </ConfirmDialog>
  );
}

/** Confirming a forced push: what would be lost on the remote. */
function ForcePushDialog() {
  const { t } = useTranslation();
  const dialog = useRemoteStore((s) => s.dialog);
  const close = useRemoteStore((s) => s.closeDialog);
  const confirm = useRemoteStore((s) => s.confirmForcePush);
  if (dialog?.kind !== "forcePush") return null;
  const { preview } = dialog;
  const more = preview.overwritten - preview.commits.length;
  if (preview.overwritten === 0) {
    return (
      <ConfirmDialog
        title={t("push.forceNotNeededTitle")}
        confirmLabel={t("toolbar.push")}
        cancelLabel={t("pull.cancel")}
        onCancel={close}
        onConfirm={() => void confirm()}
      >
        <p>{t("push.forceNotNeeded", { upstream: preview.upstream })}</p>
      </ConfirmDialog>
    );
  }
  return (
    <ConfirmDialog
      title={t("push.forceTitle", { upstream: preview.upstream })}
      confirmLabel={t("push.force")}
      cancelLabel={t("pull.cancel")}
      danger
      onCancel={close}
      onConfirm={() => void confirm()}
    >
      <p>
        {t("push.forceOverwrites", {
          count: preview.overwritten,
          upstream: preview.upstream,
        })}
      </p>
      <ul className={branches.commitList}>
        {preview.commits.map((commit) => (
          <li key={commit.oid} className={branches.commit}>
            <code className={branches.oid}>{commit.oid.slice(0, 7)}</code>
            <span className={branches.summary}>{commit.summary}</span>
            <span className={branches.author}>{commit.author}</span>
          </li>
        ))}
        {more > 0 && (
          <li className={branches.more}>
            {t("branches.moreCommits", { count: more })}
          </li>
        )}
      </ul>
      <p>{t("push.forceLease", { sha: preview.expected.slice(0, 7) })}</p>
      <p>{t("push.forceRecover")}</p>
      <pre className={`${branches.command} selectable`}>
        {t("push.forceRecoverCommand", { sha: preview.expected })}
      </pre>
    </ConfirmDialog>
  );
}
