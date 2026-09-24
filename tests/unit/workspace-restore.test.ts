import { describe, expect, it } from "vitest";
import {
  activeWorkspaceKey,
  openWorkspaceEntries,
  resolveRestoredActive
} from "../../src/lib/repositories/tabs";
import type { RepoSnapshot } from "../../src/lib/ipc/types";

function snapshot(repoId: string, workspaceKey: string): RepoSnapshot {
  return {
    repoId,
    workspaceKey,
    version: 1,
    displayName: repoId,
    displayPath: `/repos/${repoId}`,
    head: { kind: "branch", refId: "refs/heads/main", name: "main", oid: "abc" },
    trust: "trusted",
    state: "normal",
    mergeOrigin: null,
    upstream: null,
    lastFetchAt: null,
    activeOperation: null,
    stagedCount: 0,
    unstagedCount: 0,
    conflictCount: 0
  };
}

describe("open workspace persistence", () => {
  it("serializes tabs in order with stable keys", () => {
    const tabs = [
      { snapshot: snapshot("a", "worktree-v1:01") },
      { snapshot: snapshot("b", "worktree-v1:02") }
    ];
    expect(openWorkspaceEntries(tabs)).toEqual([
      { key: "worktree-v1:01", displayPath: "/repos/a" },
      { key: "worktree-v1:02", displayPath: "/repos/b" }
    ]);
  });

  it("tracks the active workspace key", () => {
    const tabs = [{ snapshot: snapshot("a", "worktree-v1:01") }];
    expect(activeWorkspaceKey(tabs, "a")).toBe("worktree-v1:01");
    expect(activeWorkspaceKey(tabs, "missing")).toBeNull();
    expect(activeWorkspaceKey([], null)).toBeNull();
  });

  it("restores the saved active tab, else the first opened", () => {
    const opened = [snapshot("a", "worktree-v1:01"), snapshot("b", "worktree-v1:02")];
    expect(resolveRestoredActive(opened, "worktree-v1:02")).toBe("b");
    expect(resolveRestoredActive(opened, "worktree-v1:gone")).toBe("a");
    expect(resolveRestoredActive([], null)).toBeNull();
  });
});
