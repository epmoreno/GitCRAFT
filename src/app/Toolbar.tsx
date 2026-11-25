import { isMac } from "./platform";
import { useRef } from "react";
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
import {
  PublishButton,
  usePublishAction,
} from "../features/publish/PublishButton";
import { useSettingsStore } from "../features/settings/store";
import { UndoButtons, useUndoActions } from "../features/undo/UndoButtons";
import type { MenuItem } from "../ui/ContextMenu";
import { Tooltip } from "../ui/Tooltip";
import styles from "./App.module.css";
import { ToolbarMenu } from "./ToolbarMenu";
import { useShortcutLabel } from "./useShortcutLabel";
import { useToolbarFit, type ToolbarPart } from "./useToolbarFit";

/**
 * Top bar. Doubles as the window title bar on macOS (overlay title bar),
 * so empty areas are drag regions.
 */
export function Toolbar() {
  // The toolbar is the title bar on macOS (drag it to move the window);
  // Windows and Linux have the system's own title bar above it.
  const dragRegion = isMac() ? { "data-tauri-drag-region": true } : {};
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

  const canBranch = !!active && !!head?.oid;
  const branchReason = !active
    ? t("toolbar.needsRepo")
    : head?.oid
      ? t("branches.newFromHead")
      : t("branches.needsCommit");
  const newBranch = () => {
    if (!active || !head?.oid) return;
    requestCreate(
      active.id,
      head.oid,
      head.type === "branch" ? head.name : head.oid.slice(0, 7),
    );
  };
  const newStash = () => {
    if (active && canStash) requestStash(active.id);
  };

  const bar = useRef<HTMLElement>(null);
  const middle = useRef<HTMLDivElement>(null);
  const dropped = useToolbarFit(bar, middle);
  const publish = usePublishAction(active?.id ?? null);
  const undoActions = useUndoActions(active?.id ?? null);
  const inMenu: Partial<Record<ToolbarPart, MenuItem[]>> = {
    publish: publish
      ? [
          {
            label: t("publish.button"),
            disabled: publish.why !== null,
            hint: publish.why ?? undefined,
            onSelect: publish.run,
          },
        ]
      : [],
    stash: [
      {
        label: t("toolbar.stash"),
        disabled: !canStash,
        hint: stashReason,
        onSelect: newStash,
      },
    ],
    branch: [
      {
        label: t("toolbar.branch"),
        disabled: !canBranch,
        hint: branchReason,
        onSelect: newBranch,
      },
    ],
    operations: [{ label: t("operations.button"), onSelect: toggleOperations }],
    undo: undoActions.map((action) => ({
      label: action.label,
      disabled: !action.enabled,
      hint: action.hint,
      onSelect: action.run,
    })),
  };

  return (
    <header ref={bar} className={styles.toolbar} {...dragRegion}>
      <span className={styles.brand} data-part="brand" {...dragRegion}>
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
      <div ref={middle} className={styles.toolbarActions} {...dragRegion}>
        <CurrentBranch repoId={active?.id ?? null} />
        <FetchButton repoId={active?.id ?? null} />
        <PullButton repoId={active?.id ?? null} />
        <PushButton repoId={active?.id ?? null} />
        <span className={styles.part} data-part="publish">
          <PublishButton repoId={active?.id ?? null} />
        </span>
        <span className={styles.part} data-part="branch">
          <Tooltip label={branchReason}>
            <button
              type="button"
              className={styles.toolButton}
              disabled={!canBranch}
              onClick={newBranch}
            >
              {t("toolbar.branch")}
            </button>
          </Tooltip>
        </span>
        <span className={styles.part} data-part="stash">
          <Tooltip label={stashReason}>
            <button
              type="button"
              className={styles.toolButton}
              disabled={!canStash}
              onClick={newStash}
            >
              {t("toolbar.stash")}
            </button>
          </Tooltip>
        </span>
        <span className={styles.part} data-part="undo">
          <UndoButtons
            repoId={active?.id ?? null}
            className={styles.toolButton}
          />
        </span>
        <span className={styles.part} data-part="menu">
          <ToolbarMenu
            items={dropped.flatMap((part) => inMenu[part] ?? [])}
            busy={runningCommands > 0 && dropped.includes("operations")}
          />
        </span>
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
      <span className={styles.part} data-part="operations">
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
      </span>
    </header>
  );
}
