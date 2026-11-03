import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import appStyles from "../app/App.module.css";
import { Tooltip } from "./Tooltip";
import styles from "./MoreOptionsButton.module.css";

/**
 * The chevron next to a toolbar button that opens its menu of options.
 * Same clickable size as a small button, with a "More … options" tooltip.
 */
export function MoreOptionsButton({
  label,
  action,
  disabled,
  onClick,
}: {
  /** Accessible name of the menu ("Fetch options"). */
  label: string;
  /** Name of the button it belongs to ("Fetch"), for the tooltip. */
  action: string;
  disabled?: boolean;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
}) {
  const { t } = useTranslation();
  return (
    <Tooltip label={t("toolbar.moreOptions", { action })}>
      <button
        type="button"
        className={`${appStyles.toolButton} ${styles.button}`}
        aria-label={label}
        aria-haspopup="menu"
        disabled={disabled}
        onClick={onClick}
      >
        <svg
          className={styles.chevron}
          viewBox="0 0 12 12"
          aria-hidden="true"
          focusable="false"
        >
          <path d="M2.5 4.5 6 8l3.5-3.5" />
        </svg>
      </button>
    </Tooltip>
  );
}
