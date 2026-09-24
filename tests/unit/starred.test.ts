import { describe, expect, it } from "vitest";
import { loadStarred, saveStarred, sortStarred, type StarStorage } from "../../src/lib/refs/starred";
import type { RefItem } from "../../src/lib/ipc/types";

function ref(refId: string, label: string): RefItem {
  return {
    refId,
    fullName: refId,
    label,
    kind: "local",
    oid: "abc",
    current: false,
    checkedOutElsewhere: false
  };
}

function memoryStorage(seed?: Record<string, unknown>): StarStorage & { dump(): unknown } {
  let raw: string | null = seed ? JSON.stringify(seed) : null;
  return {
    getItem: () => raw,
    setItem: (_key: string, value: string) => {
      raw = value;
    },
    dump: () => (raw ? JSON.parse(raw) : null)
  };
}

describe("sortStarred", () => {
  it("floats starred refs to the top keeping backend order", () => {
    const refs = [ref("a", "main"), ref("b", "feature"), ref("c", "hotfix")];
    expect(sortStarred(refs, new Set(["c", "a"])).map((r) => r.refId)).toEqual(["a", "c", "b"]);
  });

  it("leaves order untouched without stars", () => {
    const refs = [ref("a", "main"), ref("b", "feature")];
    expect(sortStarred(refs, new Set()).map((r) => r.refId)).toEqual(["a", "b"]);
  });
});

describe("starred persistence", () => {
  it("round-trips per scope without touching other scopes", () => {
    const storage = memoryStorage({ other: ["x"] });
    saveStarred("repo-a", ["b", "a"], storage);
    expect(loadStarred("repo-a", storage)).toEqual(["b", "a"]);
    expect(loadStarred("repo-b", storage)).toEqual([]);
    expect((storage.dump() as Record<string, unknown>)["other"]).toEqual(["x"]);
  });

  it("reads corrupt or missing data as empty", () => {
    const broken: StarStorage = {
      getItem: () => "{nope",
      setItem: () => {}
    };
    expect(loadStarred("repo-a", broken)).toEqual([]);
    expect(loadStarred("repo-a", null)).toEqual([]);
    expect(loadStarred("repo-a", memoryStorage())).toEqual([]);
  });
});
