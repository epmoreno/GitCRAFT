import type { RepoRefs } from "../../ipc";

export type RefLabel = {
  text: string;
  kind: "head" | "local" | "remote" | "tag";
  /** The branch HEAD is on. */
  current: boolean;
};

const ORDER: Record<RefLabel["kind"], number> = {
  head: 0,
  local: 1,
  remote: 2,
  tag: 3,
};

/** Labels per commit id: detached HEAD, then the current branch, other
 * local branches, remote branches and tags. */
export function labelsByCommit(refs: RepoRefs | null): Map<string, RefLabel[]> {
  const map = new Map<string, RefLabel[]>();
  if (!refs) return map;
  const add = (oid: string, label: RefLabel) => {
    const list = map.get(oid);
    if (list) list.push(label);
    else map.set(oid, [label]);
  };

  if (refs.head.type === "detached") {
    add(refs.head.oid, { text: "HEAD", kind: "head", current: true });
  }
  for (const ref of refs.refs) {
    add(ref.target, {
      text: ref.shortName,
      kind: ref.kind,
      current: ref.isHead,
    });
  }
  for (const list of map.values()) {
    list.sort(
      (a, b) =>
        ORDER[a.kind] - ORDER[b.kind] ||
        Number(b.current) - Number(a.current) ||
        a.text.localeCompare(b.text),
    );
  }
  return map;
}

/** The commit HEAD points to, if any. */
export function headOid(refs: RepoRefs | null): string | null {
  if (!refs) return null;
  return refs.head.type === "detached"
    ? refs.head.oid
    : (refs.head.oid ?? null);
}
