import { describe, expect, it } from "vitest";
import { mockAdapter } from "../../src/lib/ipc/mock";

describe("sync adapter (T11 demo surface)", () => {
  it("reports a read-only remote context with upstream and counts", async () => {
    const status = await mockAdapter.remoteStatus("demo");
    expect(status.remoteName).toBe("origin");
    expect(status.upstreamRef).toBe("origin/main");
    expect(status.ahead).toBe(1);
    expect(status.behind).toBe(2);
    expect(status.url).toContain("https://");
    expect(status.lastFetchAt).toMatch(/Z$/);
  });

  it("starts fetch/pull/push jobs with distinct operation ids", async () => {
    const fetch = await mockAdapter.remoteFetch("demo", 0, null);
    const pull = await mockAdapter.remotePull("demo", 0);
    const push = await mockAdapter.remotePush("demo", 0, null, true);
    for (const started of [fetch, pull, push]) {
      expect(started.operationId).toMatch(/^demo-/);
    }
    expect(new Set([fetch.operationId, pull.operationId, push.operationId]).size).toBe(3);
  });

  it("serves the operation log newest-first with redacted summaries", async () => {
    const page = await mockAdapter.operationLog("demo", 0);
    expect(page.nextCursor).toBe(0);
    expect(page.entries.length).toBeGreaterThan(0);
    const seqs = page.entries.map((e) => e.seq);
    expect([...seqs].sort((a, b) => b - a)).toEqual(seqs);
    for (const entry of page.entries) {
      expect(["ok", "error", "cancelled"]).toContain(entry.outcome);
      expect(entry.summary).not.toMatch(/token|password|credential/i);
    }
  });

  it("resolves demo jobs through the shared operation record", async () => {
    const started = await mockAdapter.remoteFetch("demo", 0, null);
    const record = await mockAdapter.operationGet(started.operationId);
    expect(record.state).toBe("succeeded");
  });
});
