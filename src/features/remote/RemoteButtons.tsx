import { useState } from "react";
import { useTranslation } from "react-i18next";
import appStyles from "../../app/App.module.css";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { useRemoteStore } from "./store";
import styles from "./remote.module.css";

type Menu = { x: number; y: number; items: MenuItem[]; label: string };

/** Opens `items` under the button that was clicked. */
function useButtonMenu() {
  const [menu, setMenu] = useState<Menu | null>(null);
  const open = (button: HTMLElement, label: string, items: MenuItem[]) => {
    const rect = button.getBoundingClientRect();
    setMenu({ x: rect.left, y: rect.bottom + 2, items, label });
  };
  const element = menu && (
    <ContextMenu
      x={menu.x}
      y={menu.y}
      label={menu.label}
      items={menu.items}
      onClose={() => setMenu(null)}
    />
  );
  return { open, element };
}

/** "Fetch" in the toolbar: every remote, or one, with or without --prune. */
export function FetchButton({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const remotes = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs?.remotes ?? null),
  );
  const running = useRemoteStore((s) => s.running);
  const fetch = useRemoteStore((s) => s.fetch);
  const menu = useButtonMenu();
  const hasRemotes = !!remotes && remotes.length > 0;
  const disabled = repoId === null || !hasRemotes || running !== null;
  const reason =
    repoId === null
      ? t("toolbar.needsRepo")
      : !hasRemotes
        ? t("remote.noRemotes")
        : running
          ? t("remote.busy")
          : t("remote.fetchHint");

  const items = (id: number): MenuItem[] => [
    {
      label: t("remote.fetchAll"),
      onSelect: () => void fetch(id, null, false),
    },
    {
      label: t("remote.fetchAllPrune"),
      onSelect: () => void fetch(id, null, true),
    },
    ...(remotes ?? []).map((remote) => ({
      label: t("remote.fetchOne", { remote }),
      onSelect: () => void fetch(id, remote, false),
    })),
  ];

  return (
    <span className={styles.split}>
      <Tooltip label={reason}>
        <button
          type="button"
          className={appStyles.toolButton}
          disabled={disabled}
          onClick={() => {
            if (repoId !== null) void fetch(repoId, null, false);
          }}
        >
          {t("toolbar.fetch")}
        </button>
      </Tooltip>
      <button
        type="button"
        className={`${appStyles.toolButton} ${styles.menuButton}`}
        aria-label={t("remote.fetchOptions")}
        aria-haspopup="menu"
        disabled={disabled}
        onClick={(event) => {
          if (repoId !== null)
            menu.open(
              event.currentTarget,
              t("remote.fetchOptions"),
              items(repoId),
            );
        }}
      >
        ▾
      </button>
      {menu.element}
    </span>
  );
}
