import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { CommandError, ErrorKind } from "../ipc";
import i18n from "../i18n";
import { osKey } from "../app/platform";
import { copyText } from "./clipboard";
import styles from "./ErrorReport.module.css";

/** Every kind the backend can report; the compiler keeps it complete, and a
 * test checks each one has a translated explanation. */
const KINDS = {
  gitNotFound: true,
  pathNotFound: true,
  notARepository: true,
  bareRepository: true,
  dubiousOwnership: true,
  identityUnknown: true,
  nothingToCommit: true,
  emptyMessage: true,
  staleDiff: true,
  undoConflict: true,
  undoUnsupported: true,
  trashFailed: true,
  invalidBranchName: true,
  branchExists: true,
  branchChanged: true,
  branchInWorktree: true,
  stashChanged: true,
  nothingToStash: true,
  authFailed: true,
  detachedHead: true,
  alreadyARepository: true,
  destinationNotEmpty: true,
  remoteUnreachable: true,
  remoteNotFound: true,
  indexLocked: true,
  localChangesOverwritten: true,
  unrelatedHistories: true,
  notFastForward: true,
  diskFull: true,
  filePermission: true,
  noCommits: true,
  hasRemote: true,
  invalidRepoName: true,
  ghRepoExists: true,
  ghNotFound: true,
  ghNotLoggedIn: true,
  ghUnreachable: true,
  ghFailed: true,
  cancelled: true,
  timedOut: true,
  unknown: true,
} satisfies Record<ErrorKind, true>;

export const ERROR_KINDS = Object.keys(KINDS) as ErrorKind[];

/** The clear explanation of `error`. Messages that name a folder use the
 * `_noPath` variant when there is none to name; those that say how to
 * install something have a variant per system. */
export function explainError(
  t: TFunction,
  error: CommandError,
  path?: string,
): string {
  const key = osKey(`errors.${error.kind}`, (k) => i18n.exists(k));
  return path ? t(key, { path }) : t(key, { context: "noPath" });
}

/** git's original output, or the technical message when git said nothing
 * (an unrecognised error always has something to show). */
export function errorOutput(error: CommandError): string | null {
  return error.details ?? (error.kind === "unknown" ? error.message : null);
}

/** How every error is shown: what failed (`lead`), why and what to do
 * (from the catalog), then git's original output on demand, copyable. */
export function ErrorReport({
  error,
  lead,
  path,
  children,
  className,
}: {
  error: CommandError;
  /** What was being done, e.g. "Couldn't push." */
  lead?: string;
  /** Folder the error is about, for messages that name it. */
  path?: string;
  /** Extra notes after the explanation. */
  children?: ReactNode;
  className?: string;
}) {
  const { t } = useTranslation();
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");
  const output = errorOutput(error);
  return (
    <div
      className={[styles.report, className].filter(Boolean).join(" ")}
      role="alert"
    >
      <p className={styles.message}>
        {lead && <strong className={styles.lead}>{lead} </strong>}
        {explainError(t, error, path)}
      </p>
      {children}
      {output && (
        <details className={styles.details}>
          <summary>{t("errors.showDetails")}</summary>
          <pre className={`${styles.output} selectable`}>{output}</pre>
          <button
            type="button"
            className={styles.copy}
            onClick={() =>
              void copyText(output).then((ok) =>
                setCopy(ok ? "copied" : "failed"),
              )
            }
          >
            {copy === "copied"
              ? t("errors.copied")
              : copy === "failed"
                ? t("errors.copyFailed")
                : t("errors.copy")}
          </button>
        </details>
      )}
    </div>
  );
}
