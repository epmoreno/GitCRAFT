import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import type { Direction, StackTop } from "../../ipc";
import { isShortcut } from "../../app/shortcuts";
import { useShortcutLabel } from "../../app/useShortcutLabel";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";
import { describeOp } from "./describe";
import { useUndoStore } from "./store";

const DIRECTIONS: Direction[] = ["undo", "redo"];

/** Text fields keep ⌘Z / Ctrl+Z for their own text. */
function editable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

/**
 * "Undo" and "Redo" in the toolbar (task 11.4), and their shortcuts. Each
 * opens the dialog with the preview; the tooltip says what, or why not.
 */
export function UndoButtons({
  repoId,
  className,
}: {
  repoId: number | null;
  className?: string;
}) {
  const { t } = useTranslation();
  const label = useShortcutLabel();
  const tops = useUndoStore((s) => (repoId === null ? null : s.tops[repoId]));
  const refresh = useUndoStore((s) => s.refresh);
  const request = useUndoStore((s) => s.request);
  // Asked again whenever the refs or the status change (an operation here
  // or outside GitCRAFT).
  const refs = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs ?? null),
  );
  const version = useWorkdirStore((s) =>
    repoId === null ? 0 : (s.statuses[repoId]?.version ?? 0),
  );
  useEffect(() => {
    if (repoId !== null) void refresh(repoId);
  }, [repoId, refs, version, refresh]);

  useEffect(() => {
    if (repoId === null) return;
    const onKey = (event: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return;
      if (editable(event.target)) return;
      const direction = isShortcut("undo", event)
        ? "undo"
        : isShortcut("redo", event)
          ? "redo"
          : null;
      if (!direction) return;
      event.preventDefault();
      void request(repoId, direction);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId, request]);

  const hint = (direction: Direction, top: StackTop | null | undefined) => {
    if (repoId === null) return t("toolbar.needsRepo");
    if (!top) return t(`undo.toolbar.nothing.${direction}`);
    const what = describeOp(t, top.kind);
    if (top.refused?.type === "inProgress") {
      return t("undo.toolbar.inProgress");
    }
    if (top.refused) return t(`undo.toolbar.changed.${direction}`, { what });
    return t(`undo.toolbar.hint.${direction}`, {
      what,
      shortcut: label(direction),
    });
  };

  return (
    <>
      {DIRECTIONS.map((direction) => {
        const top = tops?.[direction];
        const enabled = repoId !== null && !!top && !top.refused;
        return (
          <Tooltip key={direction} label={hint(direction, top)}>
            <button
              type="button"
              className={className}
              disabled={!enabled}
              onClick={() => {
                if (enabled && repoId !== null) void request(repoId, direction);
              }}
            >
              {t(`undo.toolbar.${direction}`)}
            </button>
          </Tooltip>
        );
      })}
    </>
  );
}
