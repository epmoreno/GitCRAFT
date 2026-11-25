import type { WorkingStatus } from "../../ipc";
import { useGraphStore, type OpenFile } from "../graph/store";

const AREAS = ["unstaged", "staged", "untracked"] as const;

type WorktreeFile = Extract<OpenFile, { kind: "worktree" }>;

/** Where an open uncommitted change is now: the same one, the same file in
 * another list (it was staged or unstaged whole), or `null` when it is no
 * longer a change (discarded, committed, deleted elsewhere). */
export function whereIsNow(
  file: WorktreeFile,
  status: WorkingStatus,
): WorktreeFile | null {
  if (status[file.area].some((e) => e.path === file.path)) return file;
  for (const area of AREAS) {
    const entry = status[area].find((e) => e.path === file.path);
    if (entry) return { ...file, area, oldPath: entry.oldPath };
  }
  return null;
}

/** With each new status, the change open over the history follows its file,
 * or closes: never the empty diff of a file that is no longer there. */
export function followOpenFile(repoId: number, status: WorkingStatus) {
  const graph = useGraphStore.getState();
  const file = graph.graphs[repoId]?.openFile;
  if (file?.kind !== "worktree") return;
  const now = whereIsNow(file, status);
  if (now === null) graph.closeFile(repoId);
  else if (now !== file) graph.openFileDiff(repoId, now);
}
