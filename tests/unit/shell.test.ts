import { describe, expect, it } from "vitest";
import {
  clampWidth,
  initialShell,
  nextInspector,
  SHELL_LIMITS
} from "../../src/lib/state/shell";
import { demoCommits } from "../../src/mocks/demoRepo";

describe("shell helpers (T02)", () => {
  it("clamps pane widths to spec limits", () => {
    expect(clampWidth(50, SHELL_LIMITS.sidebarMin, SHELL_LIMITS.sidebarMax)).toBe(180);
    expect(clampWidth(500, SHELL_LIMITS.sidebarMin, SHELL_LIMITS.sidebarMax)).toBe(300);
    expect(clampWidth(240, SHELL_LIMITS.sidebarMin, SHELL_LIMITS.sidebarMax)).toBe(240);
    expect(clampWidth(Number.NaN, 180, 300)).toBe(180);
    expect(clampWidth(100, SHELL_LIMITS.inspectorMin, SHELL_LIMITS.inspectorMax)).toBe(320);
    expect(clampWidth(900, SHELL_LIMITS.inspectorMin, SHELL_LIMITS.inspectorMax)).toBe(520);
  });

  it("cycles the three inspector states", () => {
    expect(nextInspector("working")).toBe("commit");
    expect(nextInspector("commit")).toBe("conflict");
    expect(nextInspector("conflict")).toBe("working");
  });

  it("starts with spec default widths", () => {
    const shell = initialShell();
    expect(shell.sidebarWidth).toBe(220);
    expect(shell.inspectorWidth).toBe(380);
    expect(shell.inspector).toBe("working");
  });
});

describe("demo fixture invariants (T02)", () => {
  it("every commit parent resolves within the fixture or is a known tip", () => {
    const oids = new Set(demoCommits.map((c) => c.oid));
    const shortIds = new Set(demoCommits.map((c) => c.shortOid));
    for (const commit of demoCommits) {
      for (const parent of commit.parents) {
        const known =
          oids.has(parent) || [...oids].some((oid) => oid.startsWith(parent) || parent.startsWith(oid.slice(0, 8)));
        expect(known || shortIds.has(parent)).toBe(true);
      }
    }
  });

  it("merge commit has two parents, root has none", () => {
    const merge = demoCommits.find((c) => c.merge);
    expect(merge?.parents).toHaveLength(2);
    expect(demoCommits.filter((c) => c.parents.length === 0)).toHaveLength(1);
  });
});
