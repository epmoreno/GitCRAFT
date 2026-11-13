import type { CommitActions } from "../../ipc";
import { availability, type CommitAction } from "./commitActions";

const facts = (overrides: Partial<CommitActions> = {}): CommitActions => ({
  oid: "a".repeat(40),
  branch: "main",
  inProgress: null,
  isMerge: false,
  inBranch: false,
  isHead: false,
  ...overrides,
});

const why = (action: CommitAction, f: CommitActions) => {
  const a = availability(action, f);
  return a.enabled ? "ok" : a.reason;
};

describe("which actions a commit offers", () => {
  it("cherry-picks only commits not yet in the branch", () => {
    expect(why("cherryPick", facts())).toBe("ok");
    expect(why("cherryPick", facts({ inBranch: true }))).toBe(
      "alreadyInBranch",
    );
  });

  it("reverts only commits of the branch", () => {
    expect(why("revert", facts({ inBranch: true }))).toBe("ok");
    expect(why("revert", facts())).toBe("notInBranch");
  });

  it("never cherry-picks or reverts a merge", () => {
    expect(why("cherryPick", facts({ isMerge: true }))).toBe("merge");
    expect(why("revert", facts({ isMerge: true, inBranch: true }))).toBe(
      "merge",
    );
  });

  it("does not reset a branch to the commit it is on", () => {
    expect(why("reset", facts({ inBranch: true }))).toBe("ok");
    expect(why("reset", facts({ isHead: true, inBranch: true }))).toBe(
      "isHead",
    );
  });

  it("changes no branch while an operation waits or HEAD is detached", () => {
    for (const action of ["cherryPick", "revert", "reset"] as const) {
      expect(why(action, facts({ inProgress: "merge" }))).toBe("inProgress");
      expect(why(action, facts({ branch: null }))).toBe("detached");
    }
  });

  it("can always create a tag", () => {
    expect(
      why("createTag", facts({ branch: null, inProgress: "rebase" })),
    ).toBe("ok");
  });
});
