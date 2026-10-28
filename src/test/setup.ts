import "@testing-library/jest-dom/vitest";
import "../i18n";
import { useGraphStore } from "../features/graph/store";
import { initialRepoState, useRepoStore } from "../features/repo/store";
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
  useGraphStore.setState({ graphs: {} });
});
