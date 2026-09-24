import { describe, expect, it } from "vitest";
import { mockAdapter, resetMockBranches, resetMockWorktree } from "../../src/lib/ipc/mock";

describe("mock commit flow (T10)", () => {
  it("commits exactly the index and keeps the draft contract", async () => {
    resetMockWorktree("commit-repo");
    const before = await mockAdapter.repoStatus("commit-repo");
    const noteId = before.files.find((f) => f.displayPath === "new notes.txt")?.pathId ?? "";
    await mockAdapter.indexStage("commit-repo", 1, [noteId]);

    const result = await mockAdapter.commitCreate("commit-repo", 1, "Demo commit", "");
    expect(result.oid).toMatch(/^[0-9a-f]{40}$/);
    // Staged-only rows vanish; nothing unstaged is swept in.
    const after = await mockAdapter.repoStatus("commit-repo");
    expect(after.files.find((f) => f.displayPath === "new notes.txt")).toBeUndefined();
    expect(after.files.find((f) => f.displayPath === "src/app/App.svelte")?.worktreeStatus).toBe("M");

    // Empty subject refuses before touching the index.
    await expect(mockAdapter.commitCreate("commit-repo", 1, "   ", "")).rejects.toMatchObject({
      code: "INVALID_ARGUMENT"
    });
    // Drain the index, then the commit refuses with EMPTY_INDEX.
    const drained = await mockAdapter.repoStatus("commit-repo");
    await mockAdapter.indexUnstage(
      "commit-repo",
      1,
      drained.files.map((f) => f.pathId)
    );
    await expect(mockAdapter.commitCreate("commit-repo", 1, "x", "")).rejects.toMatchObject({
      code: "EMPTY_INDEX"
    });
    resetMockWorktree("commit-repo");
  });

  it("reads an effective identity without secrets", async () => {
    const identity = await mockAdapter.identityRead("commit-repo");
    expect(identity.name).toBeTruthy();
    expect(identity.email).toContain("@");
    expect(identity.scope).toBe("local");
    expect(JSON.stringify(identity)).not.toMatch(/token|secret|key/i);
  });
});

describe("mock branch workflow (T10)", () => {
  it("creates, switches, confirms, and safely deletes", async () => {
    resetMockBranches();
    const refs = await mockAdapter.repoRefs();
    const head = refs.find((r) => r.current) ?? refs[0];

    const created = await mockAdapter.branchCreate("b-repo", 1, "demo/next", head.oid, true);
    expect(created.switched).toBe(true);
    const afterCreate = await mockAdapter.repoRefs();
    expect(afterCreate.find((r) => r.refId === "refs/heads/demo/next")?.current).toBe(true);

    await mockAdapter.branchSwitch("b-repo", 1, "refs/heads/main", null);
    const afterSwitch = await mockAdapter.repoRefs();
    expect(afterSwitch.find((r) => r.refId === "refs/heads/main")?.current).toBe(true);

    // Unmerged branch: summary warns, delete refuses, token is consumed.
    const prepare = await mockAdapter.confirmationPrepare("b-repo", 1, "branch_delete", [
      "refs/heads/feature/ui"
    ]);
    expect(prepare.summary).toMatch(/NOT merged/);
    await expect(
      mockAdapter.branchDelete("b-repo", 1, "refs/heads/feature/ui", prepare.confirmationToken)
    ).rejects.toMatchObject({ code: "REF_INVALID" });
    await expect(
      mockAdapter.branchDelete("b-repo", 1, "refs/heads/feature/ui", prepare.confirmationToken)
    ).rejects.toMatchObject({ code: "STALE_STATE" });

    // Duplicate names and unknown refs fail instead of guessing.
    await expect(
      mockAdapter.branchCreate("b-repo", 1, "main", head.oid, false)
    ).rejects.toMatchObject({ code: "STALE_STATE" });
    resetMockBranches();
  });
});
