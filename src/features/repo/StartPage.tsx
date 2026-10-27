import { useTranslation } from "react-i18next";
import { useRepoStore } from "./store";
import styles from "./repo.module.css";

/** Shown in the centre when no repository is open. */
export function StartPage() {
  const { t } = useTranslation();
  const recents = useRepoStore((s) => s.recents);
  const opening = useRepoStore((s) => s.opening);
  const pickAndOpen = useRepoStore((s) => s.pickAndOpen);
  const openRepository = useRepoStore((s) => s.openRepository);

  return (
    <div className={styles.start}>
      <h1 className={styles.startTitle}>{t("start.title")}</h1>
      <p className={styles.startHint}>{t("start.hint")}</p>
      <button
        type="button"
        className={styles.startButton}
        disabled={opening}
        onClick={() => void pickAndOpen(t("start.dialogTitle"))}
      >
        {t("start.open")}
      </button>

      <section className={styles.recent} aria-labelledby="recent-title">
        <h2 id="recent-title" className={styles.recentTitle}>
          {t("start.recent")}
        </h2>
        {recents.length === 0 ? (
          <p className={styles.recentEmpty}>{t("start.recentEmpty")}</p>
        ) : (
          <ul className={styles.recentList}>
            {recents.map((repo) => (
              <li key={repo.path}>
                <button
                  type="button"
                  className={styles.recentItem}
                  disabled={opening}
                  onClick={() => void openRepository(repo.path)}
                >
                  <span className={styles.recentName}>{repo.name}</span>
                  <span className={styles.recentPath}>{repo.path}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
