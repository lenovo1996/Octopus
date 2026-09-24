import { describe, expect, it } from "vitest";
import { SearchController } from "../../src/lib/search/controller";
import { mockAdapter } from "../../src/lib/ipc/mock";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("SearchController latest-query-wins (T06)", () => {
  it("keeps only the newest response when queries resolve out of order", async () => {
    const controller = new SearchController();
    const first = deferred<string>();
    const second = deferred<string>();
    const committed: string[] = [];

    const t1 = controller.begin();
    const p1 = first.promise.then((value) => {
      if (controller.isCurrent(t1)) committed.push(value);
    });
    const t2 = controller.begin();
    const p2 = second.promise.then((value) => {
      if (controller.isCurrent(t2)) committed.push(value);
    });

    // Older query resolves last; it must be discarded.
    second.resolve("new");
    first.resolve("stale");
    await Promise.all([p1, p2]);
    expect(committed).toEqual(["new"]);
  });

  it("drops in-flight queries when the repo changes", async () => {
    const controller = new SearchController();
    const token = controller.begin();
    controller.invalidate(); // repo switch / close
    expect(controller.isCurrent(token)).toBe(false);
    // A query started after the switch is current again.
    expect(controller.isCurrent(controller.begin())).toBe(true);
  });

  it("issues strictly increasing tokens", () => {
    const controller = new SearchController();
    const a = controller.begin();
    const b = controller.begin();
    expect(b.seq).toBeGreaterThan(a.seq);
    expect(controller.isCurrent(a)).toBe(false);
    expect(controller.isCurrent(b)).toBe(true);
  });
});

describe("mock historySearch (T06 demo path)", () => {
  it("matches subject/author/oid literally and paginates by offset cursor", async () => {
    const scope = { type: "allRefs" } as const;
    const page1 = await mockAdapter.historySearch("demo", scope, "nguyen", null, 1);
    expect(page1.rows.length).toBe(1);
    expect(page1.rows[0].authorName).toContain("Nguyen");
    expect(page1.nextCursor).not.toBeNull();

    const page2 = await mockAdapter.historySearch("demo", scope, "nguyen", page1.nextCursor, 10);
    expect(page2.rows.length).toBeGreaterThan(0);
    expect(page2.rows.every((r) => r.oid !== page1.rows[0].oid)).toBe(true);
  });

  it("treats regex metacharacters as literal text", async () => {
    const scope = { type: "allRefs" } as const;
    const results = await mockAdapter.historySearch("demo", scope, "feature/ui (main)", null, 10);
    expect(results.rows.length).toBe(0);
    expect(results.nextCursor).toBeNull();
  });

  it("returns an empty page for a blank query", async () => {
    const scope = { type: "allRefs" } as const;
    const results = await mockAdapter.historySearch("demo", scope, "   ", null, 10);
    expect(results.rows).toEqual([]);
    expect(results.nextCursor).toBeNull();
  });
});
