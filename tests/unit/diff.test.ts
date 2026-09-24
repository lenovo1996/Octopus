import { describe, expect, it } from "vitest";
import { mockAdapter, resetMockWorktree } from "../../src/lib/ipc/mock";

describe("mock diffRead contract (T08)", () => {
  it("returns a text document covering every line kind", async () => {
    const doc = await mockAdapter.diffRead("demo-repo", {
      kind: "worktree",
      pathId: "demo-repo:1:0"
    });
    expect(doc.kind).toBe("text");
    expect(doc.hunks.length).toBeGreaterThan(0);
    const kinds = new Set(doc.hunks.flatMap((h) => h.lines.map((l) => l.kind)));
    expect(kinds).toEqual(new Set(["context", "add", "delete", "noNewline"]));
    for (const hunk of doc.hunks) {
      expect(hunk.header.startsWith("@@")).toBe(true);
      for (const line of hunk.lines) {
        // Line numbers stay consistent with the row kind.
        if (line.kind === "add") expect(line.oldLine).toBeNull();
        if (line.kind === "delete") expect(line.newLine).toBeNull();
        if (line.kind === "context") {
          expect(line.oldLine).not.toBeNull();
          expect(line.newLine).not.toBeNull();
        }
      }
    }
  });

  it("stages and unstages exactly the selected rows", async () => {
    const mock = mockAdapter;
    resetMockWorktree("stage-repo");
    const before = await mock.repoStatus("stage-repo");
    const appRow = before.files.find((f) => f.displayPath === "src/app/App.svelte");
    const notesRow = before.files.find((f) => f.displayPath === "new notes.txt");
    expect(appRow?.worktreeStatus).toBe("M");
    expect(notesRow?.worktreeStatus).toBe("?");

    // Stage only the untracked note: index gains A, nothing else moves.
    const staged = await mock.indexStage("stage-repo", 1, [notesRow?.pathId ?? ""]);
    expect(staged.stagedCount).toBe(3);
    const mid = await mock.repoStatus("stage-repo");
    expect(mid.files.find((f) => f.displayPath === "new notes.txt")).toMatchObject({
      indexStatus: "A",
      worktreeStatus: " "
    });
    expect(mid.files.find((f) => f.displayPath === "src/app/App.svelte")?.worktreeStatus).toBe("M");

    // Unstage it back: the note returns to untracked, bytes untouched.
    await mock.indexUnstage("stage-repo", 1, [notesRow?.pathId ?? ""]);
    const after = await mock.repoStatus("stage-repo");
    expect(after.files.find((f) => f.displayPath === "new notes.txt")).toMatchObject({
      indexStatus: " ",
      worktreeStatus: "?"
    });

    // Unknown tokens and empty selections fail instead of guessing.
    await expect(mock.indexStage("stage-repo", 1, ["nope"])).rejects.toMatchObject({
      code: "STALE_STATE"
    });
    await expect(mock.indexStage("stage-repo", 1, [])).rejects.toMatchObject({
      code: "STALE_STATE"
    });
    resetMockWorktree("stage-repo");
  });

  it("requires confirmation for discard and uses backend hunk ids", async () => {
    const repoId = "discard-repo";
    resetMockWorktree(repoId);
    const before = await mockAdapter.repoStatus(repoId);
    const tracked = before.files.find((file) => file.displayPath === "src/app/App.svelte");
    const untracked = before.files.find((file) => file.worktreeStatus === "?");
    const diff = await mockAdapter.diffRead(repoId, { kind: "worktree", pathId: tracked?.pathId ?? "" });
    const hunkId = diff.hunks[0].hunkId;
    expect(hunkId).toBe("demo-hunk-1");

    await mockAdapter.diffHunkStage(repoId, 1, tracked?.pathId ?? "", hunkId);
    const hunkConfirm = await mockAdapter.confirmationPrepare(repoId, 1, "discard_hunk", [tracked?.pathId ?? "", hunkId]);
    await mockAdapter.diffHunkDiscard(repoId, 1, tracked?.pathId ?? "", hunkId, hunkConfirm.confirmationToken);
    expect((await mockAdapter.repoStatus(repoId)).files.find((file) => file.pathId === tracked?.pathId)?.worktreeStatus).toBe(" ");

    const fileConfirm = await mockAdapter.confirmationPrepare(repoId, 1, "discard_file", [untracked?.pathId ?? ""]);
    await expect(mockAdapter.worktreeDiscardFile(repoId, 1, untracked?.pathId ?? "", "wrong-token")).rejects.toMatchObject({ code: "STALE_STATE" });
    await mockAdapter.worktreeDiscardFile(repoId, 1, untracked?.pathId ?? "", fileConfirm.confirmationToken);
    expect((await mockAdapter.repoStatus(repoId)).files.some((file) => file.pathId === untracked?.pathId)).toBe(false);
    resetMockWorktree(repoId);
  });

  it("issues path ids with commit details files", async () => {
    const details = await mockAdapter.commitDetails("demo-repo", "nonexistent", null);
    expect(details.files.length).toBeGreaterThan(0);
    for (const file of details.files) {
      expect(file.pathId.length).toBeGreaterThan(0);
    }
    const commit = await mockAdapter.diffRead("demo-repo", {
      kind: "commit",
      oid: details.oid,
      parentIndex: details.parentIndex,
      pathId: details.files[0].pathId
    });
    expect(commit.kind).toBe("text");
  });
});
