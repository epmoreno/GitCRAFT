import { useTranslation } from "react-i18next";
import { useBranchStore } from "../features/branches/store";
import { NewRepositoryMenu } from "../features/create/NewRepositoryMenu";
import { useGraphStore } from "../features/graph/store";
import {
  CurrentBranch,
  FetchButton,
  PullButton,
  PushButton,
} from "../features/remote/RemoteButtons";
import { useOperationsStore } from "../features/operations/store";
import { useActiveRepository, useRepoStore } from "../features/repo/store";
import { useStashStore } from "../features/stash/store";
import { useWorkdirStore } from "../features/workdir/store";
import { PublishButton } from "../features/publish/PublishButton";
import { useSettingsStore } from "../features/settings/store";
import { Tooltip } from "../ui/Tooltip";
import styles from "./App.module.css";
import { useShortcutLabel } from "./useShortcutLabel";

const PENDING = ["undo"] as const;

/**
 * Top bar. Doubles as the window title bar on macOS (overlay title bar),
 * so empty areas are drag regions.
 */
export function Toolbar() {
  const { t } = useTranslation();
  const label = useShortcutLabel();
  const openSettings = useSettingsStore((s) => s.openSettings);
  const active = useActiveRepository();
  const opening = useRepoStore((s) => s.opening);
  const pickAndOpen = useRepoStore((s) => s.pickAndOpen);
  const requestCreate = useBranchStore((s) => s.requestCreate);
  const head = useGraphStore((s) =>
    active ? (s.graphs[active.id]?.refs?.head ?? null) : null,
  );
  const requestStash = useStashStore((s) => s.requestCreate);
  const operationsOpen = useOperationsStore((s) => s.open);
  const toggleOperations = useOperationsStore((s) => s.toggle);
  const runningCommands = useOperationsStore(
    (s) => s.records.filter((r) => !r.outcome).length,
  );
  const status = useWorkdirStore((s) =>
    active ? (s.statuses[active.id]?.status ?? null) : null,
  );
  const hasChanges =
    !!status &&
    status.staged.length + status.unstaged.length + status.untracked.length > 0;
  const stashReason = !active
    ? t("toolbar.needsRepo")
    : !head?.oid
      ? t("stash.needsCommit")
      : status && status.conflicted.length > 0
        ? t("stash.resolveFirst")
        : hasChanges
          ? t("stash.createHint")
          : t("stash.noChanges");
  const canStash =
    !!active && !!head?.oid && hasChanges && status?.conflicted.length === 0;

  // The other actions do not exist yet; explain why they are disabled.
  const disabledReason = active
    ? t("toolbar.notYetAvailable")
    : t("toolbar.needsRepo");

  return (
    <header className={styles.toolbar} data-tauri-drag-region>
      <span className={styles.brand} data-tauri-drag-region>
        GitCRAFT
      </span>
      <button
        type="button"
        className={styles.primaryButton}
        disabled={opening}
        onClick={() => void pickAndOpen(t("start.dialogTitle"))}
      >
        {t("toolbar.openRepo")}
      </button>
      <NewRepositoryMenu />
      <div className={styles.toolbarActions} data-tauri-drag-region>
        <CurrentBranch repoId={active?.id ?? null} />
        <FetchButton repoId={active?.id ?? null} />
        <PullButton repoId={active?.id ?? null} />
        <PushButton repoId={active?.id ?? null} />
        <PublishButton repoId={active?.id ?? null} />
        <Tooltip
          label={
            !active
              ? t("toolbar.needsRepo")
              : head?.oid
                ? t("branches.newFromHead")
                : t("branches.needsCommit")
          }
        >
          <button
            type="button"
            className={styles.toolButton}
            disabled={!active || !head?.oid}
            onClick={() => {
              if (!active || !head?.oid) return;
              requestCreate(
                active.id,
                head.oid,
                head.type === "branch" ? head.name : head.oid.slice(0, 7),
              );
            }}
          >
            {t("toolbar.branch")}
          </button>
        </Tooltip>
        <Tooltip label={stashReason}>
          <button
            type="button"
            className={styles.toolButton}
            disabled={!canStash}
            onClick={() => {
              if (active && canStash) requestStash(active.id);
            }}
          >
            {t("toolbar.stash")}
          </button>
        </Tooltip>
        {PENDING.map((action) => (
          <Tooltip key={action} label={disabledReason}>
            <button type="button" className={styles.toolButton} disabled>
              {t(`toolbar.${action}`)}
            </button>
          </Tooltip>
        ))}
      </div>
      <Tooltip label={t("settings.openHint", { shortcut: label("settings") })}>
        <button
          type="button"
          className={`${styles.toolButton} ${styles.iconButton}`}
          aria-label={t("settings.title")}
          onClick={() => openSettings()}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
            <path d="M13.92 7.04 L13.92 8.96 L12.48 9.06 L11.92 10.41 L12.86 11.51 L11.51 12.86 L10.41 11.92 L9.06 12.48 L8.96 13.92 L7.04 13.92 L6.94 12.48 L5.59 11.92 L4.49 12.86 L3.14 11.51 L4.08 10.41 L3.52 9.06 L2.08 8.96 L2.08 7.04 L3.52 6.94 L4.08 5.59 L3.14 4.49 L4.49 3.14 L5.59 4.08 L6.94 3.52 L7.04 2.08 L8.96 2.08 L9.06 3.52 L10.41 4.08 L11.51 3.14 L12.86 4.49 L11.92 5.59 L12.48 6.94 Z" />
            <circle cx="8" cy="8" r="2" />
          </svg>
        </button>
      </Tooltip>
      <Tooltip label={t("operations.hint")}>
        <button
          type="button"
          className={styles.toolButton}
          aria-pressed={operationsOpen}
          onClick={toggleOperations}
        >
          {t("operations.button")}
          {runningCommands > 0 && (
            <span
              className={styles.busy}
              aria-label={t("operations.runningCount", {
                count: runningCommands,
              })}
            >
              {" "}
              ●
            </span>
          )}
        </button>
      </Tooltip>
    </header>
  );
}
