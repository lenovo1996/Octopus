import { describe, expect, it } from "vitest";
import { clampMenuPosition, firstEnabledIndex, isContextMenuKey, nextEnabledIndex } from "../../src/lib/context-menu/model";

describe("context menu model", () => {
  it("keeps the menu inside every viewport edge", () => {
    expect(clampMenuPosition(790, 590, 220, 180, 800, 600)).toEqual({left:572,top:412});
    expect(clampMenuPosition(-20, -10, 220, 180, 800, 600)).toEqual({left:8,top:8});
  });

  it("moves through enabled items and wraps around disabled entries", () => {
    const items = [{disabled:true},{disabled:false},{disabled:true},{disabled:false}];
    expect(firstEnabledIndex(items)).toBe(1);
    expect(nextEnabledIndex(items,1,1)).toBe(3);
    expect(nextEnabledIndex(items,3,1)).toBe(1);
    expect(nextEnabledIndex(items,1,-1)).toBe(3);
  });

  it("returns no focus target when every item is disabled", () => {
    const items = [{disabled:true},{disabled:true}];
    expect(firstEnabledIndex(items)).toBe(-1);
    expect(nextEnabledIndex(items,0,1)).toBe(-1);
  });

  it("recognizes the keyboard context-menu gestures", () => {
    expect(isContextMenuKey({key:"ContextMenu",shiftKey:false})).toBe(true);
    expect(isContextMenuKey({key:"F10",shiftKey:true})).toBe(true);
    expect(isContextMenuKey({key:"F10",shiftKey:false})).toBe(false);
  });
});
