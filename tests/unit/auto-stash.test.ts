import { describe, expect, it } from "vitest";
import { autoStashMessage, needsStashOffer } from "../../src/lib/repositories/tabs";

describe("auto-stash offer", () => {
  it("offers only when changes exist without conflicts", () => {
    expect(needsStashOffer(0, 0, 0)).toBe(false);
    expect(needsStashOffer(1, 0, 0)).toBe(true);
    expect(needsStashOffer(0, 2, 0)).toBe(true);
    expect(needsStashOffer(1, 1, 1)).toBe(false);
  });

  it("names the switch target in the stash message", () => {
    expect(autoStashMessage("feature/ui")).toBe(
      "GitDock auto-stash before switching to feature/ui"
    );
  });
});
