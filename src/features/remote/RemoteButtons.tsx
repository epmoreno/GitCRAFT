import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import appStyles from "../../app/App.module.css";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { useRemoteStore } from "./store";
import { arrows, trackingDescription } from "./tracking";
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

/** The current branch and how far it is from its upstream. */
export function CurrentBranch({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const refs = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs ?? null),
  );
  if (!refs) return null;
  if (refs.head.type === "detached") {
    return (
      <span className={styles.branch} title={t("tracking.detached")}>
        {t("sidebar.detachedHead")}
      </span>
    );
  }
  const name = refs.head.name;
  const ref = refs.refs.find((r) => r.kind === "local" && r.shortName === name);
  const counts = ref && !ref.upstreamGone ? arrows(ref) : "";
  const description = ref ? trackingDescription(ref, t) : t("tracking.unborn");
  return (
    <span
      className={styles.branch}
      title={description}
      aria-label={`${name}. ${description}`}
    >
      <span className={styles.branchName}>{name}</span>
      {counts && <span className={styles.counts}>{counts}</span>}
      {ref && !ref.upstream && (
        <span className={styles.noUpstream}>{t("tracking.local")}</span>
      )}
      {ref?.upstreamGone && (
        <span className={styles.noUpstream}>{t("tracking.goneShort")}</span>
      )}
    </span>
  );
}

/** "Pull" in the toolbar, with the merge/rebase choice in its menu. */
export function PullButton({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const running = useRemoteStore((s) => s.running);
  const pull = useRemoteStore((s) => s.pull);
  const info = useRemoteStore((s) =>
    repoId === null ? null : (s.pullModes[repoId] ?? null),
  );
  const loadPullMode = useRemoteStore((s) => s.loadPullMode);
  const setPullMode = useRemoteStore((s) => s.setPullMode);
  const head = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs?.head ?? null),
  );
  const menu = useButtonMenu();

  useEffect(() => {
    if (repoId !== null) void loadPullMode(repoId);
  }, [repoId, loadPullMode]);

  const onBranch = head?.type === "branch" && head.oid !== null;
  const disabled = repoId === null || !onBranch || running !== null;
  const mode = info?.mode ?? "merge";
  const reason =
    repoId === null
      ? t("toolbar.needsRepo")
      : !onBranch
        ? t("remote.pullNeedsBranch")
        : running
          ? t("remote.busy")
          : t(`remote.pullHint.${mode}`);

  const items = (id: number): MenuItem[] => {
    const check = (on: boolean) => (on ? "✓ " : "   ");
    return [
      {
        label: `${check(info?.chosen === true && mode === "merge")}${t("remote.pullMerge")}`,
        onSelect: () => void setPullMode(id, "merge"),
      },
      {
        label: `${check(info?.chosen === true && mode === "rebase")}${t("remote.pullRebase")}`,
        onSelect: () => void setPullMode(id, "rebase"),
      },
      {
        label: `${check(info?.chosen === false)}${t("remote.pullFromConfig", {
          mode: t(`remote.mode.${info?.configured ?? "merge"}`),
        })}`,
        onSelect: () => void setPullMode(id, null),
      },
    ];
  };

  return (
    <span className={styles.split}>
      <Tooltip label={reason}>
        <button
          type="button"
          className={appStyles.toolButton}
          disabled={disabled}
          onClick={() => {
            if (repoId !== null) void pull(repoId);
          }}
        >
          {t("toolbar.pull")}
          {mode === "rebase" && (
            <span className={styles.mode}> ({t("remote.mode.rebase")})</span>
          )}
        </button>
      </Tooltip>
      <button
        type="button"
        className={`${appStyles.toolButton} ${styles.menuButton}`}
        aria-label={t("remote.pullOptions")}
        aria-haspopup="menu"
        disabled={repoId === null || running !== null}
        onClick={(event) => {
          if (repoId !== null)
            menu.open(
              event.currentTarget,
              t("remote.pullOptions"),
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
