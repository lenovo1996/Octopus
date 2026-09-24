import { describe, expect, it } from "vitest";
import { neighborRowIndex } from "../../src/lib/components/file-row-nav";

describe("neighborRowIndex", () => {
  it("moves one row with ArrowUp/ArrowDown and stops at the ends", () => {
    expect(neighborRowIndex(3, 0, "ArrowDown")).toBe(1);
    expect(neighborRowIndex(3, 1, "ArrowDown")).toBe(2);
    expect(neighborRowIndex(3, 2, "ArrowDown")).toBeNull();
    expect(neighborRowIndex(3, 2, "ArrowUp")).toBe(1);
    expect(neighborRowIndex(3, 0, "ArrowUp")).toBeNull();
  });

  it("jumps with Home/End and ignores other keys", () => {
    expect(neighborRowIndex(4, 2, "Home")).toBe(0);
    expect(neighborRowIndex(4, 2, "End")).toBe(3);
    expect(neighborRowIndex(4, 0, "Home")).toBeNull();
    expect(neighborRowIndex(4, 3, "End")).toBeNull();
    expect(neighborRowIndex(4, 1, "Enter")).toBeNull();
    expect(neighborRowIndex(4, 1, " ")).toBeNull();
  });

  it("rejects out-of-range or empty listings", () => {
    expect(neighborRowIndex(0, 0, "ArrowDown")).toBeNull();
    expect(neighborRowIndex(3, -1, "ArrowDown")).toBeNull();
    expect(neighborRowIndex(3, 3, "ArrowUp")).toBeNull();
  });
});
