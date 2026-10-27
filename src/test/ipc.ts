import {
  commands,
  pickFolder,
  type CommandError,
  type ErrorKind,
  type OpenedRepository,
} from "../ipc";

/** Typed handles to the mocked IPC module (see setup.ts). */
export const ipc = {
  openRepository: vi.mocked(commands.openRepository),
  closeRepository: vi.mocked(commands.closeRepository),
  listRecentRepositories: vi.mocked(commands.listRecentRepositories),
  pickFolder: vi.mocked(pickFolder),
};

export function resetIpcMock() {
  for (const fn of Object.values(ipc)) fn.mockReset();
  ipc.listRecentRepositories.mockResolvedValue({ status: "ok", data: [] });
  ipc.closeRepository.mockResolvedValue({ status: "ok", data: true });
  ipc.pickFolder.mockResolvedValue(null);
}

export function opened(id: number, path: string): OpenedRepository {
  const name = path.split("/").pop() ?? path;
  return { id, info: { path, name, gitDir: `${path}/.git` } };
}

export function commandError(
  kind: ErrorKind,
  details: string | null = null,
): CommandError {
  return { kind, message: `${kind} (test)`, details };
}
