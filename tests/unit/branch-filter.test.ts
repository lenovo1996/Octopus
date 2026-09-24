import { describe, expect, it } from "vitest";
import { filterRefs, isRefsSectionOpen, suggestedTrackName } from "../../src/lib/refs/filter";
import type { RefItem } from "../../src/lib/ipc/types";

function ref(label: string): RefItem {
  return {
    refId: `refs/heads/${label}`,
    fullName: `refs/heads/${label}`,
    label,
    kind: "local",
    oid: "abc",
    current: false,
    checkedOutElsewhere: false
  };
}

describe("sidebar branch filter", () => {
  it("keeps everything on an empty query", () => {
    const refs = [ref("main"), ref("feature/ui")];
    expect(filterRefs(refs, "")).toEqual(refs);
    expect(filterRefs(refs, "   ")).toEqual(refs);
  });

  it("matches label substrings case-insensitively", () => {
    const refs = [ref("main"), ref("feature/UI"), ref("release")];
    expect(filterRefs(refs, "ui").map((r) => r.label)).toEqual(["feature/UI"]);
    expect(filterRefs(refs, "EAT").map((r) => r.label)).toEqual(["feature/UI"]);
  });

  it("suggests a tracking name without the remote prefix", () => {
    expect(suggestedTrackName("origin/feature/ui")).toBe("feature/ui");
    expect(suggestedTrackName("origin")).toBe("origin");
  });

  it("keeps collapsed sections closed except while filtering", () => {
    expect(isRefsSectionOpen(false, false)).toBe(true);
    expect(isRefsSectionOpen(true, false)).toBe(false);
    expect(isRefsSectionOpen(true, true)).toBe(true);
    expect(isRefsSectionOpen(false, true)).toBe(true);
  });
});
