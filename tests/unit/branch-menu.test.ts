import { describe, expect, it, vi } from "vitest";
import { buildBranchMenuItems, canCheckoutRef, defaultBranchTab, directMergeSubject, pullRequestTargetName, resolveCheckoutTarget, shouldAutoCompleteMerge, shouldConfirmResetBeforeCheckout, shouldPullAfterCheckout } from "../../src/lib/refs/branch-menu";
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

const ctx = { actionsDisabled: false, selectedCommitOid: "bbb", mergeTarget: "main", mergeBusy: false };

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
      "create-pr",
      "copy-name",
      "copy-sha",
      "reveal",
      "delete"
    ]);
  });

  it("offers pull-request creation toward the clicked branch", () => {
    const local = buildBranchMenuItems(ref(), ctx, vi.fn(), vi.fn());
    expect(local.find((item) => item.id === "create-pr")?.label).toBe(
      "Create pull request to feature…"
    );
    const remote = buildBranchMenuItems(
      ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" }),
      ctx,
      vi.fn(),
      vi.fn()
    );
    const ids = remote.map((item) => item.id);
    expect(ids).toContain("create-pr");
    expect(remote.find((item) => item.id === "create-pr")?.label).toBe(
      "Create pull request to origin/feature…"
    );
  });

  it("disables pull-request creation on the current branch", () => {
    const items = buildBranchMenuItems(ref({ current: true }), ctx, vi.fn(), vi.fn());
    expect(items.find((item) => item.id === "create-pr")?.disabled).toBe(true);
    const other = buildBranchMenuItems(ref(), ctx, vi.fn(), vi.fn());
    expect(other.find((item) => item.id === "create-pr")?.disabled).toBe(false);
  });

  it("checks out the local twin instead of opening the branches dialog", () => {
    const local = ref();
    const remote = ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" });
    expect(resolveCheckoutTarget([local, remote], local)).toEqual({ refId: local.refId, trackAs: null });
    expect(resolveCheckoutTarget([local, remote], remote)).toEqual({ refId: local.refId, trackAs: null });
    expect(resolveCheckoutTarget([remote], remote)).toEqual({ refId: remote.refId, trackAs: "feature" });
  });

  it("derives the provider target name without the remote prefix", () => {
    expect(pullRequestTargetName(ref())).toBe("feature");
    expect(
      pullRequestTargetName(
        ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" })
      )
    ).toBe("feature");
    expect(
      pullRequestTargetName(
        ref({ kind: "remote", refId: "refs/remotes/upstream/release/1.0", fullName: "refs/remotes/upstream/release/1.0", label: "upstream/release/1.0" })
      )
    ).toBe("release/1.0");
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

  it("names both branches on the merge entry", () => {
    const items = buildBranchMenuItems(ref(), ctx, vi.fn(), vi.fn());
    const merge = items.find((item) => item.id === "merge");
    expect(merge?.label).toBe("Merge feature into main");
    expect(merge?.disabled).toBe(false);
  });

  it("disables merge into itself, while busy, and without a target", () => {
    const self = buildBranchMenuItems(ref({ current: true }), ctx, vi.fn(), vi.fn());
    expect(self.find((item) => item.id === "merge")?.disabled).toBe(true);
    const busy = buildBranchMenuItems(ref(), { ...ctx, mergeBusy: true }, vi.fn(), vi.fn());
    expect(busy.find((item) => item.id === "merge")?.disabled).toBe(true);
    const noTarget = buildBranchMenuItems(ref(), { ...ctx, mergeTarget: null }, vi.fn(), vi.fn());
    const fallback = noTarget.find((item) => item.id === "merge");
    expect(fallback?.disabled).toBe(true);
    expect(fallback?.label).toBe("Merge into current: feature");
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
      { actionsDisabled: true, selectedCommitOid: "bbb", mergeTarget: "main", mergeBusy: false },
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

  it("auto-completes only clean direct merges", () => {
    expect(shouldAutoCompleteMerge(true, false, false)).toBe(true);
    expect(shouldAutoCompleteMerge(true, true, false)).toBe(false);
    expect(shouldAutoCompleteMerge(true, false, true)).toBe(false);
    expect(shouldAutoCompleteMerge(false, false, false)).toBe(false);
  });

  it("builds the one-click merge subject within the backend cap", () => {
    expect(directMergeSubject("main", "dev")).toBe("Merge main into dev");
    expect(directMergeSubject("a".repeat(400), "b".repeat(400)).length).toBeLessThanOrEqual(500);
  });

  it("double-click checks out branches but never tags", () => {
    expect(canCheckoutRef(ref())).toBe(true);
    expect(canCheckoutRef(ref({ kind: "remote" }))).toBe(true);
    expect(canCheckoutRef(ref({ kind: "tag" }))).toBe(false);
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

  it("pulls after checkout only for trusted remote checkouts", () => {
    const remote = ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" });
    expect(shouldPullAfterCheckout(remote, true)).toBe(true);
    expect(shouldPullAfterCheckout(remote, false)).toBe(false);
    expect(shouldPullAfterCheckout(ref(), true)).toBe(false);
    expect(shouldPullAfterCheckout(ref({ kind: "tag" }), true)).toBe(false);
  });

  it("never auto-pulls the remote HEAD symref checkout", () => {
    const head = ref({ kind: "remote", refId: "refs/remotes/origin/HEAD", fullName: "refs/remotes/origin/HEAD", label: "origin/HEAD" });
    expect(shouldPullAfterCheckout(head, true)).toBe(false);
  });

  it("opens the Branches dialog on the create form only for create-here", () => {
    expect(defaultBranchTab(true)).toBe("create");
    expect(defaultBranchTab(false)).toBe("local");
  });

  it("confirms reset only when the trusted twin is ahead of its remote", () => {
    const remote = ref({ kind: "remote", refId: "refs/remotes/origin/feature", fullName: "refs/remotes/origin/feature", label: "origin/feature" });
    expect(shouldConfirmResetBeforeCheckout(remote, true, true, 2)).toBe(true);
    expect(shouldConfirmResetBeforeCheckout(remote, true, true, 0)).toBe(false);
    expect(shouldConfirmResetBeforeCheckout(remote, true, false, 2)).toBe(false);
    expect(shouldConfirmResetBeforeCheckout(remote, false, true, 2)).toBe(false);
    expect(shouldConfirmResetBeforeCheckout(ref(), true, true, 2)).toBe(false);
    expect(shouldConfirmResetBeforeCheckout(ref({ kind: "tag" }), true, true, 2)).toBe(false);
  });

  it("never confirms reset for the remote HEAD symref", () => {
    const head = ref({ kind: "remote", refId: "refs/remotes/origin/HEAD", fullName: "refs/remotes/origin/HEAD", label: "origin/HEAD" });
    expect(shouldConfirmResetBeforeCheckout(head, true, true, 2)).toBe(false);
  });
});
