import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ContextMenu, type MenuItem } from "../ui/ContextMenu";
import { Tooltip } from "../ui/Tooltip";
import styles from "./App.module.css";

/** "…": the toolbar actions that do not fit in the window. */
export function ToolbarMenu({
  items,
  busy,
}: {
  items: MenuItem[];
  /** A command is running and "Operations" is in here. */
  busy: boolean;
}) {
  const { t } = useTranslation();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <Tooltip label={t("toolbar.more")}>
        <button
          type="button"
          className={`${styles.toolButton} ${styles.iconButton}`}
          aria-label={t("toolbar.more")}
          aria-haspopup="menu"
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setAt({ x: rect.left, y: rect.bottom + 2 });
          }}
        >
          <span className={busy ? styles.busy : undefined}>…</span>
        </button>
      </Tooltip>
      {at && (
        <ContextMenu
          x={at.x}
          y={at.y}
          label={t("toolbar.more")}
          items={items}
          onClose={() => setAt(null)}
        />
      )}
    </>
  );
}
