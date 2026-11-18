/**
 * The interactive rebase's plan as the view edits it (task 13.4). Pure: the
 * store and the view use it, and its tests need no git.
 *
 * The rows are listed newest first, like the graph; git applies them from
 * the bottom up. "Combine" joins a commit with the kept commit below it
 * (the one applied before it): that one *receives* it.
 */

import type {
  InteractivePreview,
  PlanStep,
  RangeCommit,
  RebasePlan,
  StepAction,
} from "../../ipc";

export type Row = {
  oid: string;
  action: StepAction;
  /** "Edit message": the new message (filled with the old one). */
  message: string | null;
};

/** The message of a combined commit, edited by hand. It stays only while
 * the same commits are combined (`members`); otherwise it is filled again. */
export type Combined = Record<string, { members: string; text: string }>;

/** What a row is in a combination. */
export type Role =
  | { kind: "none" }
  /** Commits are combined into it. */
  | { kind: "receiver"; count: number }
  /** Combined into `receiver`, the kept commit below it. */
  | { kind: "combined"; receiver: string }
  /** "Combine" with nothing kept below it. */
  | { kind: "orphan" };

/** The vertical line that joins a combination, row by row. */
export type ChainLine = "none" | "start" | "middle" | "end";

/** The rows of a new view: newest first, every commit kept as it is. */
export function initialRows(preview: InteractivePreview): Row[] {
  return [...preview.commits]
    .reverse()
    .map((c) => ({ oid: c.commit.oid, action: "pick", message: null }));
}

/** Each row's role, by oid. */
export function rolesOf(rows: Row[]): Map<string, Role> {
  const roles = new Map<string, Role>();
  const counts = new Map<string, number>();
  let receiver: string | null = null;
  // In the order git applies them: from the bottom up.
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i]!;
    if (row.action === "drop") {
      roles.set(row.oid, { kind: "none" });
    } else if (row.action === "squash") {
      if (receiver === null) {
        roles.set(row.oid, { kind: "orphan" });
      } else {
        roles.set(row.oid, { kind: "combined", receiver });
        counts.set(receiver, (counts.get(receiver) ?? 0) + 1);
      }
    } else {
      receiver = row.oid;
      roles.set(row.oid, { kind: "none" });
    }
  }
  for (const [oid, count] of counts) {
    roles.set(oid, { kind: "receiver", count });
  }
  return roles;
}

/** The line joining each combination, by row (newest first). */
export function chainLines(rows: Row[], roles: Map<string, Role>): ChainLine[] {
  const lines: ChainLine[] = rows.map(() => "none");
  rows.forEach((row, end) => {
    if (roles.get(row.oid)?.kind !== "receiver") return;
    let start = end;
    rows.forEach((other, i) => {
      const role = roles.get(other.oid);
      if (role?.kind === "combined" && role.receiver === row.oid) {
        start = Math.min(start, i);
      }
    });
    for (let i = start; i <= end; i++) {
      lines[i] = i === start ? "start" : i === end ? "end" : "middle";
    }
  });
  return lines;
}

/** The commits of `receiver`'s combination, as git applies them: the
 * receiver first, then the ones combined into it from the bottom up. */
export function chainMembers(
  rows: Row[],
  roles: Map<string, Role>,
  receiver: string,
): string[] {
  const combined = rows
    .filter((row) => {
      const role = roles.get(row.oid);
      return role?.kind === "combined" && role.receiver === receiver;
    })
    .map((row) => row.oid)
    .reverse();
  return [receiver, ...combined];
}

/** The commit a row's message editor is about: its combination's receiver,
 * itself for "Edit message", or none. */
export function messageOwner(
  rows: Row[],
  roles: Map<string, Role>,
  oid: string,
): { kind: "combined" | "reword"; receiver: string } | null {
  const role = roles.get(oid);
  if (role?.kind === "receiver") return { kind: "combined", receiver: oid };
  if (role?.kind === "combined") {
    return { kind: "combined", receiver: role.receiver };
  }
  const row = rows.find((r) => r.oid === oid);
  return row?.action === "reword" ? { kind: "reword", receiver: oid } : null;
}

const byOid = (preview: InteractivePreview) =>
  new Map<string, RangeCommit>(preview.commits.map((c) => [c.commit.oid, c]));

/** The message a row has now: its new one, or the original. */
function ownMessage(row: Row, commits: Map<string, RangeCommit>): string {
  return row.action === "reword" && row.message !== null
    ? row.message
    : (commits.get(row.oid)?.message ?? "");
}

/** The message of `receiver`'s combination: the one edited by hand while
 * the same commits are combined, or else the messages of all of them, the
 * oldest first. */
export function combinedMessage(
  preview: InteractivePreview,
  rows: Row[],
  roles: Map<string, Role>,
  combined: Combined,
  receiver: string,
): string {
  const members = chainMembers(rows, roles, receiver);
  const edited = combined[receiver];
  if (edited && edited.members === members.join(" ")) return edited.text;
  const commits = byOid(preview);
  return members
    .map((oid) => {
      const row = rows.find((r) => r.oid === oid);
      return row ? ownMessage(row, commits) : "";
    })
    .join("\n\n");
}

/** Keeps a hand-edited combined message for these commits. */
export function editCombined(
  rows: Row[],
  roles: Map<string, Role>,
  combined: Combined,
  receiver: string,
  text: string,
): Combined {
  const members = chainMembers(rows, roles, receiver).join(" ");
  return { ...combined, [receiver]: { members, text } };
}

/** The plan git gets: oldest first, with each message in place. */
export function buildPlan(
  preview: InteractivePreview,
  rows: Row[],
  combined: Combined,
): RebasePlan {
  const roles = rolesOf(rows);
  const commits = byOid(preview);
  const steps: PlanStep[] = [...rows].reverse().map((row) => {
    switch (row.action) {
      case "pick":
      case "drop":
        return { oid: row.oid, action: row.action, message: null };
      case "reword":
        return {
          oid: row.oid,
          action: "reword",
          message: ownMessage(row, commits),
        };
      case "squash": {
        const role = roles.get(row.oid);
        const message =
          role?.kind === "combined"
            ? combinedMessage(preview, rows, roles, combined, role.receiver)
            : (commits.get(row.oid)?.message ?? "");
        return { oid: row.oid, action: "squash", message };
      }
    }
  });
  return { from: preview.from, steps, expected: preview.fingerprint };
}

/** `rows` with the row at `from` moved to `to` (indices, newest first). */
export function moveRow(rows: Row[], from: number, to: number): Row[] {
  if (from === to || from < 0 || from >= rows.length) return rows;
  const target = Math.max(0, Math.min(rows.length - 1, to));
  const next = [...rows];
  const [row] = next.splice(from, 1);
  next.splice(target, 0, row!);
  return next;
}

/** `rows` with `oid`'s action changed. "Edit message" starts from the
 * message the commit has. */
export function withAction(
  preview: InteractivePreview,
  rows: Row[],
  oid: string,
  action: StepAction,
): Row[] {
  const commits = byOid(preview);
  return rows.map((row) =>
    row.oid !== oid
      ? row
      : {
          oid,
          action,
          message:
            action === "reword"
              ? (row.message ?? commits.get(oid)?.message ?? "")
              : row.message,
        },
  );
}

/** Whether every commit is in its place, kept as it is. */
export function isOriginal(preview: InteractivePreview, rows: Row[]): boolean {
  const original = initialRows(preview);
  const commits = byOid(preview);
  return rows.every(
    (row, i) =>
      row.oid === original[i]?.oid &&
      (row.action === "pick" ||
        (row.action === "reword" &&
          ownMessage(row, commits) === commits.get(row.oid)?.message)),
  );
}

/** Whether the rows still list exactly the commits of `preview`. */
export function sameCommits(preview: InteractivePreview, rows: Row[]) {
  const oids = new Set(preview.commits.map((c) => c.commit.oid));
  return rows.length === oids.size && rows.every((row) => oids.has(row.oid));
}
