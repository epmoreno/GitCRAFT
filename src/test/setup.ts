import "@testing-library/jest-dom/vitest";
import "../i18n";
import { initialBranchState, useBranchStore } from "../features/branches/store";
import { useGraphStore } from "../features/graph/store";
import { initialStashState, useStashStore } from "../features/stash/store";
import { initialRepoState, useRepoStore } from "../features/repo/store";
import { useWorkdirStore } from "../features/workdir/store";
import { resetIpcMock } from "./ipc";

// No test talks to a real Tauri backend.
vi.mock("../ipc", () => ({
  commands: {
    openRepository: vi.fn(),
    closeRepository: vi.fn(),
    listRecentRepositories: vi.fn(),
    getRefs: vi.fn(),
    loadGraph: vi.fn(),
    cancelOperation: vi.fn(),
    dismissOptimizeNotice: vi.fn(),
    commitDetails: vi.fn(),
    commitChanges: vi.fn(),
    commitFileDiff: vi.fn(),
    workingStatus: vi.fn(),
    stageFiles: vi.fn(),
    unstageFiles: vi.fn(),
    createCommit: vi.fn(),
    amendInfo: vi.fn(),
    worktreeFileDiff: vi.fn(),
    applySelection: vi.fn(),
    discardPreview: vi.fn(),
    discardChanges: vi.fn(),
    undoDiscard: vi.fn(),
    checkBranchName: vi.fn(),
    createBranch: vi.fn(),
    renameBranch: vi.fn(),
    branchDeletionPreview: vi.fn(),
    deleteBranch: vi.fn(),
    checkoutBranch: vi.fn(),
    stashAndCheckout: vi.fn(),
    listStashes: vi.fn(),
    createStash: vi.fn(),
    applyStash: vi.fn(),
    dropStash: vi.fn(),
    recoverPreview: vi.fn(),
    recoverFile: vi.fn(),
  },
  pickFolder: vi.fn(),
  streamGraph: vi.fn(),
  optimizeRepository: vi.fn(),
  onRepoChanged: vi.fn(),
}));

// jsdom has no layout: give scroll containers a viewport so virtualized
// lists render rows, and no canvas (drawing is skipped without a context).
Object.defineProperty(HTMLElement.prototype, "clientHeight", {
  configurable: true,
  get: () => 600,
});
Object.defineProperty(HTMLElement.prototype, "clientWidth", {
  configurable: true,
  get: () => 1000,
});
HTMLCanvasElement.prototype.getContext = (() =>
  null) as unknown as HTMLCanvasElement["getContext"];

beforeEach(() => {
  resetIpcMock();
  useRepoStore.setState(initialRepoState);
  useBranchStore.setState(initialBranchState);
  useStashStore.setState(initialStashState);
  useGraphStore.setState({ graphs: {} });
  useWorkdirStore.setState({
    statuses: {},
    drafts: {},
    pendingDiscard: null,
    lastDiscard: null,
  });
});
