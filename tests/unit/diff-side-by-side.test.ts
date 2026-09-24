import { describe, expect, it } from "vitest";
import { pairHunkLines } from "../../src/lib/diff/side-by-side";
import type { DiffLine } from "../../src/lib/ipc/types";

function line(kind: DiffLine["kind"], text: string, oldLine: number | null = null, newLine: number | null = null): DiffLine {
  return { kind, text, oldLine, newLine };
}

describe("side-by-side pairing", () => {
  it("pairs buffered deletes with following adds", () => {
    const rows = pairHunkLines([
      line("context", "same", 1, 1),
      line("delete", "old", 2, null),
      line("add", "new", null, 2)
    ]);
    expect(rows.map((r) => r.kind)).toEqual(["both", "both"]);
    expect(rows[0].left?.text).toBe("same");
    expect(rows[1].left?.text).toBe("old");
    expect(rows[1].right?.text).toBe("new");
  });

  it("keeps unpaired deletes and adds on their own side", () => {
    const rows = pairHunkLines([
      line("delete", "gone", 1, null),
      line("context", "same", 2, 1),
      line("add", "fresh", null, 2)
    ]);
    expect(rows.map((r) => r.kind)).toEqual(["left", "both", "right"]);
  });

  it("spans no-newline markers across both columns", () => {
    const rows = pairHunkLines([line("noNewline", "\\ No newline at end of file")]);
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe("both");
  });

  it("flushes trailing deletes", () => {
    const rows = pairHunkLines([line("delete", "a", 1, null), line("delete", "b", 2, null)]);
    expect(rows.map((r) => r.kind)).toEqual(["left", "left"]);
  });
});
