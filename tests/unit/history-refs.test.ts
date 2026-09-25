import { describe, expect, it } from "vitest";
import { refItemForBadge } from "../../src/lib/history/refs";
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
