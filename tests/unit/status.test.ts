import { describe, expect, it } from "vitest";
import { mockAdapter } from "../../src/lib/ipc/mock";
import { partitionStatus, summarizeWorkingChanges } from "../../src/lib/status/partition";
import type { ChangedFile } from "../../src/lib/ipc/types";

function file(overrides: Partial<ChangedFile> & { displayPath: string }): ChangedFile {
  return {
    pathId: `demo:1:${overrides.displayPath}`,
    indexStatus: " ",
    worktreeStatus: " ",
    kind: "unknown",
    conflicted: false,
    ...overrides
  };
}

describe("partitionStatus", () => {
  it("keeps staged+unstaged edits of one file in both sections", () => {
    const { staged, unstaged } = partitionStatus([
      file({ displayPath: "a.txt", indexStatus: "M", worktreeStatus: "M" })
    ]);
    expect(staged.map((f) => f.displayPath)).toEqual(["a.txt"]);
    expect(unstaged.map((f) => f.displayPath)).toEqual(["a.txt"]);
  });

  it("counts untracked as unstaged only and ignores ignored entries", () => {
    const { staged, unstaged } = partitionStatus([
      file({ displayPath: "new.txt", indexStatus: " ", worktreeStatus: "?" }),
      file({ displayPath: "out.log", indexStatus: " ", worktreeStatus: "!" })
    ]);
    expect(staged).toEqual([]);
    expect(unstaged.map((f) => f.displayPath)).toEqual(["new.txt"]);
  });

  it("marks conflicted rows without dropping them", () => {
    const { staged, unstaged } = partitionStatus([
      file({ displayPath: "both.txt", indexStatus: "U", worktreeStatus: "U", conflicted: true })
    ]);
    expect(staged).toHaveLength(1);
    expect(unstaged).toHaveLength(1);
    expect(staged[0].conflicted).toBe(true);
  });
});

describe("summarizeWorkingChanges", () => {
  it("breaks unstaged rows into added/modified/deleted", () => {
    expect(
      summarizeWorkingChanges([
        file({ displayPath: "new.txt", worktreeStatus: "?" }),
        file({ displayPath: "edit.txt", worktreeStatus: "M" }),
        file({ displayPath: "gone.txt", worktreeStatus: "D" })
      ])
    ).toEqual({ total: 3, added: 1, modified: 1, deleted: 1 });
  });

  it("counts staged-only rows once, preferring the worktree side", () => {
    expect(
      summarizeWorkingChanges([
        file({ displayPath: "both.txt", indexStatus: "M", worktreeStatus: "M" }),
        file({ displayPath: "staged-add.txt", indexStatus: "A", worktreeStatus: " " }),
        file({ displayPath: "staged-del.txt", indexStatus: "D", worktreeStatus: " " })
      ])
    ).toEqual({ total: 3, added: 1, modified: 1, deleted: 1 });
  });

  it("hides on null, clean or conflicted-only listings", () => {
    expect(summarizeWorkingChanges(null)).toBeNull();
    expect(summarizeWorkingChanges([])).toBeNull();
    expect(
      summarizeWorkingChanges([file({ displayPath: "clean.txt", indexStatus: " ", worktreeStatus: " " })])
    ).toBeNull();
    expect(
      summarizeWorkingChanges([
        file({ displayPath: "c.txt", indexStatus: "U", worktreeStatus: "U", conflicted: true })
      ])
    ).toBeNull();
  });
});

describe("mock repoStatus contract", () => {
  it("returns ChangedFile rows with unique path ids covering both sections", async () => {
    const data = await mockAdapter.repoStatus("demo-repo");
    expect(data.files.length).toBeGreaterThan(0);
    const ids = data.files.map((f) => f.pathId);
    expect(new Set(ids).size).toBe(ids.length);
    for (const row of data.files) {
      expect(row.displayPath.length).toBeGreaterThan(0);
      expect(row.indexStatus).toHaveLength(1);
      expect(row.worktreeStatus).toHaveLength(1);
    }
    const { staged, unstaged } = partitionStatus(data.files);
    expect(staged.length + unstaged.length).toBeGreaterThanOrEqual(data.files.length);
  });
});
