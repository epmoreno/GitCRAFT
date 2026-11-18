import { create } from "zustand";
import {
  commands,
  type CommandError,
  type InteractivePreview,
  type PlanPreview,
  type StepAction,
} from "../../ipc";
import { useGraphStore } from "../graph/store";
import { useHistoryStore } from "../history/store";
import { refreshAfterRemote } from "../remote/store";
import {
  buildPlan,
  editCombined,
  initialRows,
  messageOwner,
  moveRow,
  rolesOf,
  sameCommits,
  withAction,
  type Combined,
  type Row,
} from "./plan";

/**
 * The interactive rebase view (task 13.4): the plan the user is editing,
 * its prediction, and applying it. Over the history of one tab at a time.
 */

export type RebaseView = {
  repoId: number;
  /** The commit chosen: the oldest one of the range. */
  from: string;
  preview: InteractivePreview | null;
  error: CommandError | null;
  rows: Row[];
  combined: Combined;
  selected: string | null;
  prediction: PlanPreview | null;
  predicting: boolean;
  confirming: boolean;
  /** The repository changed under the view: the preview was made again
   * (`reset`: with other commits, so the plan started over). */
  changed: null | "kept" | "reset";
};

type State = {
  view: RebaseView | null;
  busy: boolean;
  open: (repoId: number, from: string) => Promise<void>;
  close: () => void;
  select: (oid: string) => void;
  setAction: (oid: string, action: StepAction) => void;
  /** Moves a row `delta` places (negative: up, applied later). */
  move: (oid: string, delta: number) => void;
  moveTo: (from: number, to: number) => void;
  /** The message of `oid`'s editor (its own or its combination's). */
  setMessage: (oid: string, text: string) => void;
  resetPlan: () => void;
  /** Makes the preview again (the repository changed). */
  reload: () => Promise<void>;
  requestApply: () => void;
  cancelApply: () => void;
  apply: () => Promise<void>;
};

/** Waits this long after the last change before predicting. */
export const PREDICT_DELAY = 250;

let predictTimer: ReturnType<typeof setTimeout> | null = null;
let predictRun = 0;

export const initialRebaseState = { view: null, busy: false };

export const useInteractiveRebaseStore = create<State>()((set, get) => {
  const patch = (update: Partial<RebaseView>) => {
    const view = get().view;
    if (view) set({ view: { ...view, ...update } });
  };

  const predictNow = async () => {
    const view = get().view;
    if (!view?.preview || view.preview.refused) return;
    const run = ++predictRun;
    const plan = buildPlan(view.preview, view.rows, view.combined);
    const result = await commands.simulateRebasePlan(view.repoId, plan);
    if (run !== predictRun || get().view?.repoId !== view.repoId) return;
    if (result.status === "error") {
      patch({ predicting: false, error: result.error });
      return;
    }
    patch({ predicting: false, prediction: result.data, error: null });
  };

  /** Predicts again once the plan stops changing. */
  const predict = () => {
    patch({ predicting: true });
    if (predictTimer) clearTimeout(predictTimer);
    predictTimer = setTimeout(() => {
      predictTimer = null;
      void predictNow();
    }, PREDICT_DELAY);
  };

  /** A new plan: the prediction is stale until predicted again. */
  const edit = (update: Partial<RebaseView>) => {
    patch(update);
    predict();
  };

  const loadPreview = async (repoId: number, from: string) => {
    const result = await commands.interactiveRebasePreview(repoId, from);
    const view = get().view;
    if (view?.repoId !== repoId || view.from !== from) return null;
    if (result.status === "error") {
      patch({ error: result.error });
      return null;
    }
    return result.data;
  };

  /** Takes a new preview, keeping the plan if the commits are the same. */
  const takePreview = (
    preview: InteractivePreview,
    changed: RebaseView["changed"] | "detect",
  ) => {
    const view = get().view;
    if (!view) return;
    const keep = view.rows.length > 0 && sameCommits(preview, view.rows);
    const rows = keep ? view.rows : initialRows(preview);
    patch({
      preview,
      rows,
      combined: keep ? view.combined : {},
      // The chosen commit, which the graph already has selected.
      selected: keep
        ? view.selected
        : (rows.find((r) => r.oid === view.from)?.oid ?? rows[0]?.oid ?? null),
      error: null,
      changed:
        changed === "detect"
          ? view.preview && view.preview.fingerprint !== preview.fingerprint
            ? keep
              ? "kept"
              : "reset"
            : view.changed
          : changed,
    });
    if (!preview.refused && preview.commits.length > 0) predict();
  };

  return {
    ...initialRebaseState,

    open: async (repoId, from) => {
      if (predictTimer) clearTimeout(predictTimer);
      useGraphStore.getState().closeFile(repoId);
      useHistoryStore.setState({ notice: null });
      set({
        view: {
          repoId,
          from,
          preview: null,
          error: null,
          rows: [],
          combined: {},
          selected: null,
          prediction: null,
          predicting: false,
          confirming: false,
          changed: null,
        },
      });
      const preview = await loadPreview(repoId, from);
      if (preview) takePreview(preview, null);
    },

    close: () => {
      if (predictTimer) clearTimeout(predictTimer);
      predictTimer = null;
      predictRun++;
      set({ view: null });
    },

    select: (oid) => {
      const view = get().view;
      if (!view) return;
      patch({ selected: oid });
      useGraphStore.getState().select(view.repoId, oid);
    },

    setAction: (oid, action) => {
      const view = get().view;
      if (!view?.preview) return;
      edit({ rows: withAction(view.preview, view.rows, oid, action) });
    },

    move: (oid, delta) => {
      const view = get().view;
      if (!view) return;
      const index = view.rows.findIndex((r) => r.oid === oid);
      const to = index + delta;
      if (index < 0 || to < 0 || to >= view.rows.length) return;
      edit({ rows: moveRow(view.rows, index, to) });
    },

    moveTo: (from, to) => {
      const view = get().view;
      if (!view || from === to) return;
      edit({ rows: moveRow(view.rows, from, to) });
    },

    setMessage: (oid, text) => {
      const view = get().view;
      if (!view) return;
      const roles = rolesOf(view.rows);
      const owner = messageOwner(view.rows, roles, oid);
      if (!owner) return;
      if (owner.kind === "reword") {
        edit({
          rows: view.rows.map((r) =>
            r.oid === oid ? { ...r, message: text } : r,
          ),
        });
      } else {
        edit({
          combined: editCombined(
            view.rows,
            roles,
            view.combined,
            owner.receiver,
            text,
          ),
        });
      }
    },

    resetPlan: () => {
      const view = get().view;
      if (!view?.preview) return;
      edit({ rows: initialRows(view.preview), combined: {} });
    },

    reload: async () => {
      const view = get().view;
      if (!view || get().busy) return;
      const preview = await loadPreview(view.repoId, view.from);
      if (preview) takePreview(preview, "detect");
    },

    requestApply: () => {
      const view = get().view;
      if (!view?.prediction || view.predicting || view.prediction.problem) {
        return;
      }
      patch({ confirming: true });
    },

    cancelApply: () => patch({ confirming: false }),

    apply: async () => {
      const view = get().view;
      const preview = view?.preview;
      const prediction = view?.prediction;
      if (!view || !preview || !prediction || get().busy) return;
      const { repoId } = view;
      const branch = preview.branch ?? "";
      set({ busy: true });
      patch({ confirming: false });
      const result = await commands.interactiveRebase(repoId, {
        plan: buildPlan(preview, view.rows, view.combined),
        saveChanges: preview.blocked !== null,
        allowConflicts: prediction.stopsAt !== null,
      });
      set({ busy: false });
      refreshAfterRemote(repoId);
      if (result.status === "error") {
        patch({ error: result.error });
        return;
      }
      const { outcome, undoId } = result.data;
      const history = useHistoryStore.getState();
      if (outcome.saved) {
        history.rememberSaved(repoId, {
          oid: outcome.saved,
          pick: "interactiveRebase",
          short: branch,
        });
      }
      switch (outcome.kind) {
        case "done":
          get().close();
          useHistoryStore.setState({
            notice: {
              kind: "merged",
              op: "interactiveRebase",
              repoId,
              undoId,
              count: prediction.rewritten,
              published: prediction.publishedIn.length > 0,
              source: branch,
              target: branch,
              fastForward: false,
              undoGone: false,
            },
          });
          void useGraphStore
            .getState()
            .refresh(repoId)
            .then(() => useGraphStore.getState().select(repoId, outcome.head));
          return;
        case "conflicted":
          // The banner and the conflict view (task 9) take over.
          get().close();
          return;
        case "notDone":
          takePreview(outcome.preview, outcome.changed ? "detect" : null);
          return;
      }
    },
  };
});
