import { describe, expect, it } from "vitest";
import { mockAdapter } from "../../src/lib/ipc/mock";

describe("stash adapter (T12 demo surface)", () => {
  it("lists entries newest-first with OID identity", async () => {
    const entries = await mockAdapter.stashList("demo");
    expect(entries.length).toBeGreaterThan(0);
    for (const entry of entries) {
      expect(entry.stashId).toMatch(/^stash@\{\d+\}$/);
      expect(entry.oid.length).toBeGreaterThan(0);
      expect(entry.createdAt).toBeGreaterThan(0);
    }
  });

  it("saves with an explicit untracked flag and reports no-change separately", async () => {
    const saved = await mockAdapter.stashSave("demo", 1, "wip", false);
    expect(saved.noChange).toBe(false);
    expect(saved.oid).toBeTruthy();
    expect(saved.snapshot.repoId).toBeTruthy();
  });

  it("apply retains while pop drops, never conflicted in demo", async () => {
    const [first] = await mockAdapter.stashList("demo");
    const applied = await mockAdapter.stashApply("demo", 1, first.stashId, first.oid, "apply");
    expect(applied.retained).toBe(true);
    expect(applied.conflicted).toBe(false);
    expect(applied.dropError).toBeNull();
    const popped = await mockAdapter.stashApply("demo", 1, first.stashId, first.oid, "pop");
    expect(popped.retained).toBe(false);
    expect(popped.dropError).toBeNull();
  });
});
