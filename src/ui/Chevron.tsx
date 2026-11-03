import styles from "./Chevron.module.css";

/** Small chevron icon; `right` when collapsed, `down` when open or a menu. */
export function Chevron({
  direction = "down",
}: {
  direction?: "down" | "right";
}) {
  return (
    <svg
      className={
        direction === "right"
          ? `${styles.chevron} ${styles.right}`
          : styles.chevron
      }
      viewBox="0 0 12 12"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M2.5 4.5 6 8l3.5-3.5" />
    </svg>
  );
}
