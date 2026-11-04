import "@testing-library/jest-dom/vitest";
import i18n from "../i18n";
import { initialBranchState, useBranchStore } from "../features/branches/store";
import { initialCreateState, useCreateStore } from "../features/create/store";
import { useGraphStore } from "../features/graph/store";
import {
  initialSettingsState,
  useSettingsStore,
} from "../features/settings/store";
import {
  initialOperationsState,
  useOperationsStore,
} from "../features/operations/store";
import { initialRemoteState, useRemoteStore } from "../features/remote/store";
import { initialStashState, useStashStore } from "../features/stash/store";
import { initialRepoState, useRepoStore } from "../features/repo/store";
import { useViewStore } from "../features/repo/viewState";
import { useWorkdirStore } from "../features/workdir/store";
import {
  initialWorkdirViewPrefs,
  useWorkdirViewPrefs,
} from "../features/workdir/viewPrefs";
import { resetIpcMock } from "./ipc";

// The native menu bar does not exist in tests.
vi.mock("../app/menu", () => ({
  installAppMenu: vi.fn(() => Promise.resolve()),
}));

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
    getPullMode: vi.fn(),
    setPullMode: vi.fn(),
    abortInProgress: vi.fn(),
    saveSession: vi.fn(),
    restoreSession: vi.fn(),
    checkInit: vi.fn(),
    validBranchName: vi.fn(),
    initRepository: vi.fn(),
    cloneParent: vi.fn(),
    defaultCloneName: vi.fn(),
    checkCloneDestination: vi.fn(),
    cancelClone: vi.fn(),
    commandHistory: vi.fn(),
    clearCommandHistory: vi.fn(),
    generalSettings: vi.fn(),
    setGeneralSettings: vi.fn(),
    gitDefaults: vi.fn(),
    pullStrategyChanges: vi.fn(),
    setDefaultBranch: vi.fn(),
    setDefaultPullStrategy: vi.fn(),
    gitIdentity: vi.fn(),
    identityChanges: vi.fn(),
    setGitIdentity: vi.fn(),
    ghStatus: vi.fn(),
    cancelGhLogin: vi.fn(),
    ghLogout: vi.fn(),
    ghSetupGit: vi.fn(),
    publishInfo: vi.fn(),
    validRepoName: vi.fn(),
    publishRepository: vi.fn(),
  },
  setUiZoom: vi.fn(),
  ghLogin: vi.fn(),
  pickFolder: vi.fn(),
  streamGraph: vi.fn(),
  optimizeRepository: vi.fn(),
  fetchRemote: vi.fn(),
  pullBranch: vi.fn(),
  pushBranch: vi.fn(),
  forcePushPreview: vi.fn(),
  cloneRepository: vi.fn(),
  onRepoChanged: vi.fn(),
  onCommandRecord: vi.fn(),
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
  useViewStore.setState({ collapsed: {}, filters: {} });
  useBranchStore.setState(initialBranchState);
  useStashStore.setState(initialStashState);
  useRemoteStore.setState(initialRemoteState);
  useCreateStore.setState(initialCreateState);
  useOperationsStore.setState(initialOperationsState);
  useGraphStore.setState({ graphs: {} });
  useSettingsStore.setState(initialSettingsState);
  localStorage.clear();
  useWorkdirViewPrefs.setState(initialWorkdirViewPrefs);
  useWorkdirStore.setState({
    statuses: {},
    drafts: {},
    pendingDiscard: null,
    lastDiscard: null,
  });
});

// T.3: a text key used by a rendered component must exist (in "es" and, by
// the i18n test, in "en"): i18next would otherwise show the key itself.
const missingKeys: string[] = [];
i18n.options.saveMissing = true;
i18n.options.missingKeyHandler = (_lngs, _ns, key) => {
  missingKeys.push(key);
};

afterEach(() => {
  const missing = missingKeys.splice(0);
  expect(missing, "i18n keys used but not defined").toEqual([]);
});
