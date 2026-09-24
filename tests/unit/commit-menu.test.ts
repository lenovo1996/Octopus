import { describe, expect, it } from "vitest";
import { COMMIT_ACTIONS } from "../../src/lib/history/commit-menu";

describe("commit context menu", () => {
  it("keeps the requested actions and order", () => {
    expect(COMMIT_ACTIONS.map((item) => item.label)).toEqual([
      "Checkout this commit…",
      "Create branch here…",
      "Create tag here…",
      "Push to…",
      "Cherry-pick onto HEAD…",
      "Revert commit…",
      "Merge into current branch…",
      "Rebase current branch onto this…",
      "Reword message…",
      "Modify commit…",
      "Edit author…",
      "Split commit…",
      "Move to branch…",
      "Interactive rebase from here…",
      "Reset HEAD (soft) to here",
      "Reset HEAD (mixed) to here",
      "Reset HEAD (hard) to here"
    ]);
  });

  it("backs every action with a typed IPC flow", () => {
    expect(COMMIT_ACTIONS.filter((item) => item.available).map((item) => item.id)).toEqual([
      "checkout",
      "create-branch",
      "create-tag",
      "push-to",
      "cherry-pick",
      "revert",
      "merge",
      "rebase",
      "reword",
      "modify",
      "edit-author",
      "split",
      "move-to-branch",
      "interactive-rebase",
      "reset-soft",
      "reset-mixed",
      "reset-hard"
    ]);
    expect(COMMIT_ACTIONS.filter((item) => item.separatorBefore).map((item) => item.id)).toEqual([
      "cherry-pick",
      "merge",
      "reword",
      "reset-soft"
    ]);
  });

  it("marks hard reset as destructive", () => {
    expect(COMMIT_ACTIONS.find((item) => item.id === "reset-hard")?.danger).toBe(true);
  });
});
