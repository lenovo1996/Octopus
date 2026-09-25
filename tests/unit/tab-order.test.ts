import { describe, expect, it } from "vitest";
import { reorderTabs } from "../../src/lib/repositories/tabs";
import type { RepoSnapshot } from "../../src/lib/ipc/types";

const tab = (repoId: string) => ({ snapshot: { repoId } as RepoSnapshot });

describe("reorderTabs", () => {
  it("moves a tab before or after another tab", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    expect(reorderTabs(tabs, "c", "a", true).map((t) => t.snapshot.repoId)).toEqual(["c", "a", "b"]);
    expect(reorderTabs(tabs, "a", "c", false).map((t) => t.snapshot.repoId)).toEqual(["b", "c", "a"]);
    expect(reorderTabs(tabs, "a", "b", true).map((t) => t.snapshot.repoId)).toEqual(["a", "b", "c"]);
  });

  it("leaves the list untouched for unknown ids or no-op moves", () => {
    const tabs = [tab("a"), tab("b")];
    expect(reorderTabs(tabs, "x", "a", true)).toBe(tabs);
    expect(reorderTabs(tabs, "a", "x", true)).toBe(tabs);
    expect(reorderTabs(tabs, "a", "a", true)).toBe(tabs);
  });
});
