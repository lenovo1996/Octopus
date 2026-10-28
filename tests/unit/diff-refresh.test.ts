import { describe, expect, it } from "vitest";
import { resolveOpenDiffRow } from "../../src/lib/status/reopen";
import type { ChangedFile } from "../../src/lib/ipc/types";

function file(overrides: Partial<ChangedFile> & { displayPath: string }): ChangedFile {
  return {
    pathId: `repo:2:${overrides.displayPath}`,
    indexStatus: " ",
    worktreeStatus: " ",
    kind: "unknown",
    conflicted: false,
    ...overrides
  };
}

describe("resolveOpenDiffRow", () => {
  it("re-resolves an open worktree diff by display path with the fresh token", () => {
    const row = resolveOpenDiffRow(
      [file({ displayPath: "a.txt", pathId: "repo:2:0", worktreeStatus: "M" })],
      "a.txt",
      "worktree"
    );
    expect(row?.pathId).toBe("repo:2:0");
  });

  it("re-resolves an open staged diff on the index side", () => {
    const row = resolveOpenDiffRow(
      [file({ displayPath: "a.txt", pathId: "repo:2:0", indexStatus: "M", worktreeStatus: " " })],
      "a.txt",
      "index"
    );
    expect(row?.pathId).toBe("repo:2:0");
  });

  it("re-resolves untracked previews on the worktree side", () => {
    const row = resolveOpenDiffRow(
      [file({ displayPath: "new.txt", worktreeStatus: "?" })],
      "new.txt",
      "worktree"
    );
    expect(row?.displayPath).toBe("new.txt");
  });

  it("returns null when the file left the open side, so the stale view closes", () => {
    // Fully staged file: nothing left to show on the worktree side.
    expect(
      resolveOpenDiffRow(
        [file({ displayPath: "a.txt", indexStatus: "M", worktreeStatus: " " })],
        "a.txt",
        "worktree"
      )
    ).toBeNull();
    // File vanished from the listing entirely.
    expect(resolveOpenDiffRow([], "a.txt", "worktree")).toBeNull();
    expect(resolveOpenDiffRow(null, "a.txt", "index")).toBeNull();
  });

  it("never matches blank or ignored statuses", () => {
    const files = [
      file({ displayPath: "clean.txt", indexStatus: " ", worktreeStatus: " " }),
      file({ displayPath: "out.log", indexStatus: " ", worktreeStatus: "!" })
    ];
    expect(resolveOpenDiffRow(files, "clean.txt", "worktree")).toBeNull();
    expect(resolveOpenDiffRow(files, "clean.txt", "index")).toBeNull();
    expect(resolveOpenDiffRow(files, "out.log", "worktree")).toBeNull();
  });
});
