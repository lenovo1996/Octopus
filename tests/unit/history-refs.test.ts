import { describe, expect, it } from "vitest";
import { badgeLabel, branchTipPlacement, originTwin, primaryBadge, refBadge, refItemForBadge, refsByCommit, type RefBadge } from "../../src/lib/history/refs";
import type { RefItem } from "../../src/lib/ipc/types";

const refs: RefItem[] = [
  {
    refId: "refs/heads/feature",
    fullName: "refs/heads/feature",
    label: "feature",
    kind: "local",
    oid: "aaa",
    current: false,
    checkedOutElsewhere: false
  },
  {
    refId: "refs/remotes/origin/feature",
    fullName: "refs/remotes/origin/feature",
    label: "origin/feature",
    kind: "remote",
    oid: "aaa",
    current: false,
    checkedOutElsewhere: false
  }
];

describe("refItemForBadge", () => {
  it("resolves the graph badge back to its sidebar RefItem", () => {
    expect(refItemForBadge(refs, { id: "refs/heads/feature" })?.label).toBe("feature");
    expect(refItemForBadge(refs, { id: "refs/remotes/origin/feature" })?.kind).toBe("remote");
  });

  it("returns null for unknown or stale badge ids", () => {
    expect(refItemForBadge(refs, { id: "refs/heads/gone" })).toBeNull();
    expect(refItemForBadge([], { id: "refs/heads/feature" })).toBeNull();
  });
});

describe("refBadge", () => {
  it("includes the remote name in a displayed label", () => {
    expect(badgeLabel(refBadge(refs[0]))).toBe("feature");
    expect(badgeLabel(refBadge(refs[1]))).toBe("origin/feature");
  });

  it("marks the checked-out branch so the graph can fill it", () => {
    const current = refBadge({ ...refs[0], current: true });
    expect(current.current).toBe(true);
    expect(refBadge(refs[0]).current).toBe(false);
    expect(refBadge(refs[1]).current).toBe(false);
  });
});

describe("primaryBadge", () => {
  function badge(overrides: Partial<RefBadge> = {}): RefBadge {
    return {
      id: "refs/heads/main",
      name: "main",
      source: "local",
      kind: "local",
      fullName: "refs/heads/main",
      current: false,
      ...overrides
    };
  }

  it("shows the local badge when a commit carries local and remote twins", () => {
    const local = badge({ current: true });
    const origin = badge({ id: "refs/remotes/origin/main", name: "main", source: "origin", kind: "remote", fullName: "refs/remotes/origin/main" });
    expect(primaryBadge([local, origin])).toBe(local);
    expect(primaryBadge([origin, local])).toBe(local);
  });

  it("prefers locals over remotes, origin over other remotes, remotes over tags", () => {
    const upstream = badge({ id: "r-up", name: "main", source: "upstream", kind: "remote", fullName: "refs/remotes/upstream/main" });
    const origin = badge({ id: "r-or", name: "main", source: "origin", kind: "remote", fullName: "refs/remotes/origin/main" });
    const local = badge();
    const tag = badge({ id: "t", name: "v1", source: "tag", kind: "tag", fullName: "refs/tags/v1" });
    expect(primaryBadge([local, upstream])).toBe(local);
    expect(primaryBadge([upstream, origin])).toBe(origin);
    expect(primaryBadge([tag, upstream])).toBe(upstream);
    expect(primaryBadge([tag])).toBe(tag);
  });

  it("returns null with no badges", () => {
    expect(primaryBadge([])).toBeNull();
  });

  it("shows the requested remote ahead of two local branches on the same commit", () => {
    const localA = badge({ id: "refs/heads/A", name: "A", current: true });
    const originA = badge({ id: "refs/remotes/origin/A", name: "A", source: "origin", kind: "remote", fullName: "refs/remotes/origin/A" });
    const localB = badge({ id: "refs/heads/B", name: "B" });
    for (const badges of [[localA, originA, localB], [localB, localA, originA], [originA, localB, localA]]) {
      expect(primaryBadge(badges, originA.id)).toBe(originA);
      expect(primaryBadge(badges, localB.id)).toBe(localB);
      expect(badges).toHaveLength(3);
    }
  });

  it("falls back to the checked-out local branch when the preference is absent or stale", () => {
    const localA = badge({ id: "refs/heads/A", name: "A" });
    const localB = badge({ id: "refs/heads/B", name: "B", current: true });
    expect(primaryBadge([localA, localB])).toBe(localB);
    expect(primaryBadge([localA, localB], "refs/remotes/origin/deleted")).toBe(localB);
  });
});

describe("originTwin", () => {
  it("pairs the same local/origin branch in either primary direction", () => {
    const badges = refs.map(refBadge);
    expect(originTwin(badges, badges[0])).toBe(badges[1]);
    expect(originTwin(badges, badges[1])).toBe(badges[0]);
  });

  it("pairs branches with slashes and does not require the current branch", () => {
    const badges = refs.map(ref => refBadge({ ...ref, fullName: `${ref.fullName}/nested`, label: `${ref.label}/nested` }));
    expect(originTwin(badges, badges[0])).toBe(badges[1]);
    expect(badges.every(badge => !badge.current)).toBe(true);
  });

  it("does not pair diverged tips, other branch names, tags, or other remotes", () => {
    const grouped = refsByCommit([refs[0], { ...refs[1], oid: "bbb" }]);
    expect(originTwin(grouped.get("aaa")!, refBadge(refs[0]))).toBeNull();
    expect(originTwin([refBadge(refs[0]), refBadge({ ...refs[1], fullName: "refs/remotes/origin/other" })], refBadge(refs[0]))).toBeNull();
    const upstream = refBadge({ ...refs[1], fullName: "refs/remotes/upstream/feature" });
    expect(originTwin([refBadge(refs[0]), upstream], refBadge(refs[0]))).toBeNull();
    expect(originTwin([refBadge(refs[0]), upstream], upstream)).toBeNull();
    const tag = refBadge({ ...refs[0], kind: "tag", fullName: "refs/tags/feature" });
    expect(originTwin([tag, refBadge(refs[1])], tag)).toBeNull();
  });

  it("keeps unrelated refs separate and preserves the selected checkout identity", () => {
    const badges = [...refs, { ...refs[0], refId: "refs/heads/other", fullName: "refs/heads/other", label: "other", current: true }].map(refBadge);
    const primary = primaryBadge(badges, refs[1].refId)!;
    expect(primary.id).toBe(refs[1].refId);
    expect(originTwin(badges, primary)?.id).toBe(refs[0].refId);
    expect(badges).toHaveLength(3);
  });
});

describe("branchTipPlacement", () => {
  it("drops below the pill and clamps to the viewport", () => {
    expect(branchTipPlacement({ left: 100, top: 100, bottom: 120 }, 1440, 800)).toEqual({ x: 100, y: 124, above: false });
    expect(branchTipPlacement({ left: 1400, top: 100, bottom: 120 }, 1440, 800).x).toBeLessThan(1400);
  });

  it("flips above the pill near the bottom edge", () => {
    const at = branchTipPlacement({ left: 100, top: 780, bottom: 798 }, 1440, 800);
    expect(at.above).toBe(true);
    expect(at.y).toBe(776);
  });
});
