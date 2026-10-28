import { useTranslation } from "react-i18next";
import type { FileStatus } from "../../ipc";
import styles from "./workdir.module.css";

const LETTER: Record<FileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  unmerged: "U",
  untracked: "?",
  unknown: "?",
};

/** One-letter file status, named for assistive technology. */
export function StatusLetter({ status }: { status: FileStatus }) {
  const { t } = useTranslation();
  const label = t(`files.status.${status}`);
  return (
    <span
      className={`${styles.letter} ${styles[`letter-${status}`] ?? ""}`}
      title={label}
      aria-label={label}
    >
      {LETTER[status]}
    </span>
  );
}
