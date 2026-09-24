import { describe, expect, it } from "vitest";
import type { RepoSnapshot } from "../../src/lib/ipc/types";
import { headLabel } from "../../src/mocks/demoSession";
import { demoSession } from "../../src/mocks/demoSession";

describe("repository session helpers (T03)", () => {
  it("labels every HEAD kind", () => {
    expect(headLabel({ kind: "branch", refId: "refs/heads/main", name: "main", oid: "abc" })).toBe(
      "main"
    );
    expect(headLabel({ kind: "detached", oid: "abcdef123456" })).toBe("detached abcdef12");
    expect(headLabel({ kind: "unborn", name: "trunk" })).toBe("unborn trunk");
  });

  it("demo session matches the IPC snapshot contract", () => {
    const session: RepoSnapshot = demoSession;
    expect(session.repoId).toBeTruthy();
    expect(session.stagedCount).toBeNull();
    expect(session.unstagedCount).toBeNull();
    expect(session.conflictCount).toBeNull();
    expect(JSON.stringify(session)).toContain('"repoId"');
    expect(JSON.stringify(session)).not.toContain("repo_id");
  });
});
