import { describe, expect, it } from "vitest";
import { clampColumn, defaultColumns, formatCommitDate, restoreColumns } from "../../src/lib/history/columns";
import { refBadge, refsByCommit } from "../../src/lib/history/refs";
import type { RefItem } from "../../src/lib/ipc/types";

describe("history column preferences", () => {
  it("restores explicit widths while leaving the default subject responsive", () => {
    expect(restoreColumns(null)).toEqual(defaultColumns());
    const resized = { branches: 170, graph: 110, subject: 360, author: 190 };
    expect(restoreColumns(JSON.stringify(resized))).toEqual({ ...resized, date: 120 });
    expect(restoreColumns(JSON.stringify({ ...resized, date: 200 }))).toMatchObject({ date: 200 });
  });
  it("bounds corrupted widths and ignores invalid storage", () => {
    for (const raw of ["invalid", "null", "7", "[]"]) expect(restoreColumns(raw)).toEqual(defaultColumns());
    expect(restoreColumns('{"branches": -20, "subject": 99999, "author":"wide"}')).toMatchObject({branches:110,subject:1400,author:140});
    expect(clampColumn("graph", Infinity)).toBe(84);
    expect(clampColumn("date", 10)).toBe(90);
    expect(clampColumn("date", 9999)).toBe(300);
  });
  it("formats commit timestamps compactly", () => {
    const now = Date.parse("2026-09-24T12:00:00Z");
    expect(formatCommitDate("2026-09-24T11:59:30Z", now)).toBe("just now");
    expect(formatCommitDate("2026-09-24T11:15:00Z", now)).toBe("45m");
    expect(formatCommitDate("2026-09-24T09:00:00Z", now)).toBe("3h");
    expect(formatCommitDate("2026-09-22T12:00:00Z", now)).toBe("2d");
    expect(formatCommitDate("2026-09-10T12:00:00Z", now)).toBe("10 Sep");
    expect(formatCommitDate("2024-01-05T12:00:00Z", now)).toBe("5 Jan 2024");
    expect(formatCommitDate("not-a-date", now)).toBe("not-a-date");
  });
});

const ref = (kind: "local" | "remote" | "tag", fullName: string, label: string): RefItem => ({refId:fullName, fullName, label, kind, oid:"tip", current:false, checkedOutElsewhere:false});
describe("branch decorations", () => {
  it("distinguishes local and origin names on the same commit without inferring ancestry", () => {
    const refs = [ref("remote", "refs/remotes/origin/feature/ui", "origin/feature/ui"), ref("local", "refs/heads/feature/ui", "feature/ui")];
    const grouped = refsByCommit(refs);
    expect(grouped.size).toBe(1);
    expect(grouped.get("tip")?.map(r => [r.source,r.name])).toEqual([["local","feature/ui"],["origin","feature/ui"]]);
    expect(grouped.get("ancestor")).toBeUndefined();
  });
  it("keeps the real remote name and tag type", () => {
    expect(refBadge(ref("remote","refs/remotes/upstream/release/v2","upstream/release/v2"))).toMatchObject({source:"upstream",name:"release/v2",kind:"remote"});
    expect(refBadge(ref("tag","refs/tags/v1","v1"))).toMatchObject({source:"tag",name:"v1"});
  });
});
