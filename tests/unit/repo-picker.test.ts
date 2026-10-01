import { describe, expect, it } from "vitest";
import type { RecentEntry } from "../../src/lib/ipc/types";
import {
  clampPickerSelection,
  filterRecentRepos,
  formatLastOpened,
  movePickerSelection,
  repoBaseName
} from "../../src/lib/repositories/picker";

function entry(displayPath: string, entryId = displayPath): RecentEntry {
  return { entryId, key: displayPath, displayPath, lastOpenedAt: 1790000000 };
}

describe("repoBaseName", () => {
  it("takes the last segment of posix and windows paths", () => {
    expect(repoBaseName("/home/u/octopus")).toBe("octopus");
    expect(repoBaseName("C:\\repos\\octopus")).toBe("octopus");
  });

  it("tolerates trailing slashes and bare names", () => {
    expect(repoBaseName("/home/u/octopus/")).toBe("octopus");
    expect(repoBaseName("octopus")).toBe("octopus");
    expect(repoBaseName("/")).toBe("/");
  });
});

describe("filterRecentRepos", () => {
  const recents = [entry("/demo/octopus-demo"), entry("/demo/website")];

  it("keeps every entry for a blank query", () => {
    expect(filterRecentRepos(recents, "")).toBe(recents);
    expect(filterRecentRepos(recents, "   ")).toBe(recents);
  });

  it("matches case-insensitively and trims the query", () => {
    expect(filterRecentRepos(recents, "  OCTOPUS ")).toEqual([recents[0]]);
    expect(filterRecentRepos(recents, "demo/")).toEqual(recents);
    expect(filterRecentRepos(recents, "nothing")).toEqual([]);
  });
});

describe("movePickerSelection", () => {
  it("wraps around both ends", () => {
    expect(movePickerSelection(1, 1, 2)).toBe(0);
    expect(movePickerSelection(0, -1, 2)).toBe(1);
    expect(movePickerSelection(0, 1, 3)).toBe(1);
  });

  it("stays at 0 for an empty list", () => {
    expect(movePickerSelection(0, 1, 0)).toBe(0);
    expect(movePickerSelection(5, -1, 0)).toBe(0);
  });
});

describe("clampPickerSelection", () => {
  it("clamps into range and resets for an empty list", () => {
    expect(clampPickerSelection(9, 2)).toBe(1);
    expect(clampPickerSelection(-4, 2)).toBe(0);
    expect(clampPickerSelection(1, 2)).toBe(1);
    expect(clampPickerSelection(0, 0)).toBe(0);
  });
});

describe("formatLastOpened", () => {
  const now = 1790800000;

  it("labels minutes, hours, and days relatively", () => {
    expect(formatLastOpened(now - 30, now)).toBe("Just now");
    expect(formatLastOpened(now - 5 * 60, now)).toBe("5m ago");
    expect(formatLastOpened(now - 3 * 3600, now)).toBe("3h ago");
    expect(formatLastOpened(now - 86400, now)).toBe("Yesterday");
    expect(formatLastOpened(now - 4 * 86400, now)).toBe("4d ago");
  });

  it("falls back to a local date after a week", () => {
    const label = formatLastOpened(now - 10 * 86400, now);
    expect(label).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);
  });

  it("omits unknown or future stamps", () => {
    expect(formatLastOpened(0, now)).toBe("");
    expect(formatLastOpened(-5, now)).toBe("");
    expect(formatLastOpened(now + 60, now)).toBe("");
    expect(formatLastOpened(Number.NaN, now)).toBe("");
  });
});
