import { useTranslation } from "react-i18next";
import type { OpenedRepository } from "../../ipc";
import styles from "./repo.module.css";

/** Centre zone for an open repository. The commit graph replaces this. */
export function RepoView({ repo }: { repo: OpenedRepository }) {
  const { t } = useTranslation();
  return (
    <div className={styles.repoView}>
      <h1 className={styles.repoName}>{repo.info.name}</h1>
      <p className={`${styles.repoPath} selectable`}>{repo.info.path}</p>
      <p className={styles.repoPlaceholder}>{t("repo.historyPlaceholder")}</p>
    </div>
  );
}
