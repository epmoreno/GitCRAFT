import styles from "./merge.module.css";

/** Branch names longer than this are cut in the sketch (the text says
 * them in full). */
const MAX_NAME = 22;

function short(name: string) {
  return name.length > MAX_NAME ? `${name.slice(0, MAX_NAME - 1)}…` : name;
}

/**
 * A small fixed drawing of what happens (task 8): not the real graph, just
 * which branch moves. Decorative: the dialog's text says the same.
 *
 * - `fastForward`: `moving` advances along `other`'s commits.
 * - `merge`: `moving` gets a merge commit joining `other`.
 */
export function BranchSketch({
  kind,
  moving,
  other,
}: {
  kind: "fastForward" | "merge";
  moving: string;
  other: string;
}) {
  return (
    <svg
      className={styles.sketch}
      viewBox="0 0 320 76"
      role="presentation"
      aria-hidden="true"
    >
      {kind === "fastForward" ? (
        <>
          <line x1="20" y1="38" x2="200" y2="38" className={styles.edge} />
          <circle cx="20" cy="38" r="6" className={styles.commit} />
          <circle cx="80" cy="38" r="6" className={styles.commit} />
          <circle cx="140" cy="38" r="6" className={styles.commit} />
          <circle cx="200" cy="38" r="6" className={styles.commit} />
          <text x="20" y="20" className={styles.oldLabel}>
            {short(moving)}
          </text>
          <path d="M 40 14 Q 120 0 192 22" className={styles.move} />
          <text x="200" y="66" className={styles.label} textAnchor="middle">
            {short(other)}
          </text>
          <text x="214" y="42" className={styles.movingLabel}>
            ← {short(moving)}
          </text>
        </>
      ) : (
        <>
          <line x1="20" y1="22" x2="230" y2="22" className={styles.edge} />
          <path d="M 50 22 L 90 56 L 170 56 L 230 22" className={styles.edge} />
          <circle cx="20" cy="22" r="6" className={styles.commit} />
          <circle cx="50" cy="22" r="6" className={styles.commit} />
          <circle cx="130" cy="22" r="6" className={styles.commit} />
          <circle cx="90" cy="56" r="6" className={styles.commit} />
          <circle cx="170" cy="56" r="6" className={styles.commit} />
          <rect
            x="223"
            y="15"
            width="14"
            height="14"
            className={styles.newCommit}
          />
          <text x="244" y="26" className={styles.movingLabel}>
            {short(moving)}
          </text>
          <text x="182" y="70" className={styles.label}>
            {short(other)}
          </text>
        </>
      )}
    </svg>
  );
}
