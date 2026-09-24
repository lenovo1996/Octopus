import { describe, expect, it } from "vitest";
import { createMockAdapter } from "../../src/lib/ipc/mock";
import { demoSession } from "../../src/mocks/demoSession";
import {
  collectDiscardAllTargets,
  discardAllSummary
} from "../../src/lib/status/discard-all";
import type { ChangedFile } from "../../src/lib/ipc/types";

function file(overrides: Partial<ChangedFile> & { displayPath: string }): ChangedFile {
  return {
    pathId: `t:1:${overrides.displayPath}`,
    indexStatus: " ",
    worktreeStatus: "M",
    kind: "unknown",
    conflicted: false,
    ...overrides
  };
}

describe("collectDiscardAllTargets", () => {
  it("takes unstaged rows only, skips staged-only and conflicted rows", () => {
    const targets = collectDiscardAllTargets([
      file({ displayPath: "edit.txt", worktreeStatus: "M" }),
      file({ displayPath: "new.txt", worktreeStatus: "?" }),
      file({ displayPath: "staged.txt", indexStatus: "M", worktreeStatus: " " }),
      file({ displayPath: "both.txt", indexStatus: "U", worktreeStatus: "U", conflicted: true })
    ]);
    expect(targets.map((t) => t.displayPath).sort()).toEqual(["edit.txt", "new.txt"]);
    expect(targets.find((t) => t.displayPath === "new.txt")?.untracked).toBe(true);
    expect(targets.find((t) => t.displayPath === "edit.txt")?.untracked).toBe(false);
  });

  it("returns empty for null or clean listings", () => {
    expect(collectDiscardAllTargets(null)).toEqual([]);
    expect(collectDiscardAllTargets([file({ displayPath: "a.txt", worktreeStatus: " " })])).toEqual(
      []
    );
  });
});

describe("discardAllSummary", () => {
  it("warns about untracked deletes and kept staged changes", () => {
    const summary = discardAllSummary([
      { pathId: "a", displayPath: "edit.txt", untracked: false },
      { pathId: "b", displayPath: "new.txt", untracked: true }
    ]);
    expect(summary).toContain("Discard all 2 unstaged changes");
    expect(summary).toContain("untracked file is deleted");
    expect(summary).toContain("Staged changes are kept");
  });
});

describe("discard-all against the mock adapter", () => {
  it("sequential per-file prepare+discard clears every unstaged row", async () => {
    const adapter = createMockAdapter(structuredClone(demoSession));
    const before = await adapter.repoStatus("demo-repo");
    const targets = collectDiscardAllTargets(before.files);
    expect(targets.length).toBeGreaterThan(0);

    for (const target of targets) {
      const live = await adapter.repoStatus("demo-repo");
      const row = live.files.find((f) => f.displayPath === target.displayPath);
      expect(row).toBeDefined();
      const prepared = await adapter.confirmationPrepare("demo-repo", 0, "discard_file", [
        row!.pathId
      ]);
      await adapter.worktreeDiscardFile("demo-repo", 0, row!.pathId, prepared.confirmationToken);
    }

    const after = await adapter.repoStatus("demo-repo");
    expect(collectDiscardAllTargets(after.files)).toEqual([]);
  });
});
