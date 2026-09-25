import { describe, expect, it } from "vitest";
import { changedLineOrdinal } from "../../src/lib/diff/lines";
import type { DiffHunk } from "../../src/lib/ipc/types";

const hunk = {
  hunkId: "h",
  header: "@@ -1,4 +1,5 @@",
  oldStart: 1,
  newStart: 1,
  lines: [
    { kind: "context", oldLine: 1, newLine: 1, text: "a" },
    { kind: "delete", oldLine: 2, newLine: null, text: "b" },
    { kind: "add", oldLine: null, newLine: 2, text: "B" },
    { kind: "context", oldLine: 3, newLine: 3, text: "c" },
    { kind: "noNewline", oldLine: null, newLine: null, text: "" }
  ]
} as DiffHunk;

describe("changedLineOrdinal", () => {
  it("counts add/delete rows only, mirroring the backend rule", () => {
    expect(changedLineOrdinal(hunk, 0)).toBeNull();
    expect(changedLineOrdinal(hunk, 1)).toBe(0);
    expect(changedLineOrdinal(hunk, 2)).toBe(1);
    expect(changedLineOrdinal(hunk, 3)).toBeNull();
    expect(changedLineOrdinal(hunk, 4)).toBeNull();
  });
});
