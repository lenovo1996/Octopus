import { describe, expect, it, vi } from "vitest";
import { buildBranchMenuItems } from "../../src/lib/refs/branch-menu";
import type { RefItem } from "../../src/lib/ipc/types";

function ref(overrides: Partial<RefItem> = {}): RefItem {
  return {
    refId: "refs/heads/feature",
    fullName: "refs/heads/feature",
    label: "feature",
    kind: "local",
    oid: "aaa",
    current: false,
    checkedOutElsewhere: false,
    ...overrides
  };
}

const ctx = { actionsDisabled: false, selectedCommitOid: "bbb" };

describe("branch context menu", () => {
  it("orders local items like the requested desktop menu", () => {
    const items = buildBranchMenuItems(ref(), ctx, vi.fn(), vi.fn());
    expect(items.map((item) => item.id)).toEqual([
      "checkout",
      "merge",
      "rebase",
      "cherry-pick",
      "revert",
      "create-branch",
      "create-tag",
      "move",
      "reset-soft",
      "reset-mixed",
      "reset-hard",
      "rename",
      "upstream",
      "push",
      "push-to",
      "copy-name",
      "copy-sha",
      "reveal",
      "delete"
    ]);
  });

  it("keeps local-only actions out of the remote menu", () => {
    const items = buildBranchMenuItems(
      ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" }),
      ctx,
      vi.fn(),
      vi.fn()
    );
    const ids = items.map((item) => item.id);
    expect(ids).not.toContain("move");
    expect(ids).not.toContain("rename");
    expect(ids).not.toContain("upstream");
    expect(ids).not.toContain("push");
    expect(ids).not.toContain("delete");
    expect(ids).toContain("push-to");
    expect(ids).toContain("checkout");
  });

  it("disables checkout on the current branch and move without a target", () => {
    const onAction = vi.fn();
    const current = buildBranchMenuItems(ref({ current: true }), ctx, onAction, vi.fn());
    expect(current.find((item) => item.id === "checkout")?.disabled).toBe(true);
    const noTarget = buildBranchMenuItems(ref(), { ...ctx, selectedCommitOid: null }, onAction, vi.fn());
    expect(noTarget.find((item) => item.id === "move")?.disabled).toBe(true);
    const sameTarget = buildBranchMenuItems(ref(), { ...ctx, selectedCommitOid: "aaa" }, onAction, vi.fn());
    expect(sameTarget.find((item) => item.id === "move")?.disabled).toBe(true);
  });

  it("locks mutations but never copies or reveal", () => {
    const items = buildBranchMenuItems(
      ref(),
      { actionsDisabled: true, selectedCommitOid: "bbb" },
      vi.fn(),
      vi.fn()
    );
    for (const id of ["checkout", "merge", "push", "reset-hard", "delete"]) {
      expect(items.find((item) => item.id === id)?.disabled).toBe(true);
    }
    for (const id of ["copy-name", "copy-sha", "reveal", "create-branch"]) {
      expect(items.find((item) => item.id === id)?.disabled ?? false).toBe(false);
    }
  });

  it("routes clicks to the action callback", () => {
    const onAction = vi.fn();
    const onCopy = vi.fn();
    const items = buildBranchMenuItems(ref(), ctx, onAction, onCopy);
    items.find((item) => item.id === "rename")?.action?.();
    expect(onAction).toHaveBeenCalledWith("rename", expect.objectContaining({ label: "feature" }));
    items.find((item) => item.id === "copy-sha")?.action?.();
    expect(onCopy).toHaveBeenCalledWith("aaa");
  });
});
