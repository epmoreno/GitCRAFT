import { cloneElement, useId, type ReactElement } from "react";
import styles from "./Tooltip.module.css";

type Props = {
  label: string;
  /** A single element; it is described by the tooltip for assistive tech. */
  children: ReactElement<{ "aria-describedby"?: string }>;
};

/**
 * Hover/focus tooltip. Wraps the child in a span so it also works for
 * disabled buttons, which do not receive pointer events themselves.
 */
export function Tooltip({ label, children }: Props) {
  const id = useId();
  return (
    <span className={styles.host}>
      {cloneElement(children, { "aria-describedby": id })}
      <span role="tooltip" id={id} className={styles.tip}>
        {label}
      </span>
    </span>
  );
}
