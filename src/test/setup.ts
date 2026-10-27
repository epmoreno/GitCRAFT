import "@testing-library/jest-dom/vitest";
import "../i18n";
import { initialRepoState, useRepoStore } from "../features/repo/store";
import { resetIpcMock } from "./ipc";

// No test talks to a real Tauri backend.
vi.mock("../ipc", () => ({
  commands: {
    openRepository: vi.fn(),
    closeRepository: vi.fn(),
    listRecentRepositories: vi.fn(),
  },
  pickFolder: vi.fn(),
}));

beforeEach(() => {
  resetIpcMock();
  useRepoStore.setState(initialRepoState);
});
