import type { PointerEvent as ReactPointerEvent } from "react";
import { create } from "zustand";
import { isShortcut } from "../../app/shortcuts";
import type { BranchName } from "./menu";
import { useMergeStore } from "./store";

/**
 * Dragging a branch onto another (task 8.4), with pointer events rather
 * than HTML5 drag and drop (which, on Windows, Tauri's file drop takes
 * over). A drag starts once the pointer moves a few pixels with the button
 * down, so clicks and double clicks work as before. Dropping on a local
 * branch opens the merge/rebase dialog; Esc or dropping elsewhere does
 * nothing.
 *
 * Drop targets are elements with `data-drop-branch` (full ref name),
 * `data-drop-name` (short name) and `data-drop-repo`.
 */

/** Pixels the pointer must move before a press becomes a drag. */
export const DRAG_THRESHOLD = 4;
/** Distance to a scroller's edge that scrolls it while dragging. */
const EDGE = 40;
const SCROLL_STEP = 12;

type Dragged = {
  repoId: number;
  branch: BranchName;
  x: number;
  y: number;
};

type DragState = {
  dragging: Dragged | null;
  /** The drop target under the pointer, valid or not. */
  over: BranchName | null;
};

export const initialDragState: DragState = { dragging: null, over: null };

export const useDragStore = create<DragState>()(() => initialDragState);

/** Whether `target` can receive `dragging`: another local branch of the
 * same repository. */
export function canDrop(
  dragging: Dragged | null,
  repoId: number,
  target: string,
): boolean {
  return (
    !!dragging &&
    dragging.repoId === repoId &&
    target.startsWith("refs/heads/") &&
    target !== dragging.branch.name
  );
}

/** How a branch element should look while something is dragged. */
export function useDropState(
  repoId: number | null,
  refname: string,
): "target" | "over" | null {
  return useDragStore((s) => {
    if (repoId === null || !canDrop(s.dragging, repoId, refname)) return null;
    return s.over?.name === refname ? "over" : "target";
  });
}

/** The data attributes that make an element a drop target. */
export function dropTargetProps(repoId: number, branch: BranchName) {
  return {
    "data-drop-branch": branch.name,
    "data-drop-name": branch.shortName,
    "data-drop-repo": String(repoId),
  };
}

function targetAt(element: EventTarget | null): {
  repoId: number;
  branch: BranchName;
} | null {
  const found =
    element instanceof Element
      ? element.closest<HTMLElement>("[data-drop-branch]")
      : null;
  if (!found) return null;
  const { dropBranch, dropName, dropRepo } = found.dataset;
  if (!dropBranch || !dropName || !dropRepo) return null;
  return {
    repoId: Number(dropRepo),
    branch: { name: dropBranch, shortName: dropName },
  };
}

/** Scrolls the scroller (`data-drag-scroll`) whose edge the pointer is
 * near. */
function scrollNearEdge(x: number, y: number) {
  for (const el of document.querySelectorAll<HTMLElement>(
    "[data-drag-scroll]",
  )) {
    const rect = el.getBoundingClientRect();
    if (x < rect.left || x > rect.right) continue;
    if (y >= rect.top && y < rect.top + EDGE) el.scrollTop -= SCROLL_STEP;
    else if (y <= rect.bottom && y > rect.bottom - EDGE) {
      el.scrollTop += SCROLL_STEP;
    }
  }
}

/** The click that follows the drop must not select or open anything. */
function swallowNextClick() {
  const swallow = (event: MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };
  window.addEventListener("click", swallow, { capture: true, once: true });
  window.setTimeout(
    () => window.removeEventListener("click", swallow, { capture: true }),
    0,
  );
}

/** `onPointerDown` of a draggable branch. */
export function beginBranchDrag(
  event: ReactPointerEvent,
  repoId: number,
  branch: BranchName,
) {
  if (event.button !== 0) return;
  const startX = event.clientX;
  const startY = event.clientY;
  let active = false;
  let last = { x: startX, y: startY };
  let frame = 0;

  const scrollLoop = () => {
    scrollNearEdge(last.x, last.y);
    frame = window.requestAnimationFrame(scrollLoop);
  };

  const end = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", key, true);
    window.cancelAnimationFrame(frame);
    delete document.body.dataset.dragging;
    useDragStore.setState(initialDragState);
  };

  const move = (e: PointerEvent) => {
    if (!active) {
      if (Math.hypot(e.clientX - startX, e.clientY - startY) < DRAG_THRESHOLD)
        return;
      active = true;
      document.body.dataset.dragging = "branch";
      frame = window.requestAnimationFrame(scrollLoop);
    }
    e.preventDefault();
    last = { x: e.clientX, y: e.clientY };
    const target = targetAt(e.target);
    // Over something that cannot receive it: the cursor says so.
    document.body.dataset.dragging =
      target &&
      !canDrop(
        { repoId, branch, x: 0, y: 0 },
        target.repoId,
        target.branch.name,
      )
        ? "blocked"
        : "branch";
    useDragStore.setState({
      dragging: { repoId, branch, x: e.clientX, y: e.clientY },
      over: target?.repoId === repoId ? target.branch : null,
    });
  };

  const up = (e: PointerEvent) => {
    const wasActive = active;
    const target = targetAt(e.target);
    const dragging = useDragStore.getState().dragging;
    end();
    if (!wasActive) return;
    swallowNextClick();
    if (target && canDrop(dragging, target.repoId, target.branch.name)) {
      void useMergeStore
        .getState()
        .requestDrop(repoId, branch.name, target.branch.name);
    }
  };

  const cancel = () => end();

  const key = (e: KeyboardEvent) => {
    if (!isShortcut("escape", e) || !active) return;
    e.preventDefault();
    e.stopPropagation();
    end();
  };

  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", key, true);
}
