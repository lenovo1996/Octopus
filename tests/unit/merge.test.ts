import { describe, expect, it } from "vitest";
import { mockAdapter } from "../../src/lib/ipc/mock";

describe("merge adapter (T13 demo surface)", () => {
  it("starts a conflicted merge and lists one supported text conflict", async () => {
    const started = await mockAdapter.mergeStart("demo", 1, "refs/heads/feature", "demo-head");
    expect(started.conflicted).toBe(true);
    expect(started.alreadyUpToDate).toBe(false);
    const list = await mockAdapter.conflictList("demo");
    expect(list.files).toHaveLength(1);
    const [file] = list.files;
    expect(file.kind).toBe("text");
    expect(file.supported).toBe(true);
    expect(file.hasBase && file.hasCurrent && file.hasIncoming).toBe(true);
    expect(list.canAbort).toBe(true);
  });

  it("previews all three stages with labels and actions", async () => {
    const preview = await mockAdapter.conflictPreview("demo", "demo:conflict:0");
    expect(preview.base?.text).toContain("base line");
    expect(preview.current?.text).toContain("main line");
    expect(preview.incoming?.text).toContain("feature line");
    expect(preview.supportedActions).toEqual(["current", "incoming"]);
    expect(preview.currentLabel).toBeTruthy();
    expect(preview.incomingLabel).toBeTruthy();
    expect(preview.workingFingerprint).toBeTruthy();
  });

  it("accepts a side behind confirmation and completes the merge", async () => {
    const prepared = await mockAdapter.confirmationPrepare("demo", 1, "conflict_accept", [
      "demo:conflict:0",
      "incoming"
    ]);
    expect(prepared.confirmationToken).toBeTruthy();
    const accepted = await mockAdapter.conflictAccept(
      "demo",
      1,
      "demo:conflict:0",
      "incoming",
      "demofp:24",
      prepared.confirmationToken
    );
    expect(accepted.workingFingerprint).toBeTruthy();
    expect(accepted.snapshot.repoId).toBeTruthy();
    const done = await mockAdapter.mergeComplete("demo", 1, "Merge feature", "", true, "demo-head");
    expect(done.oid).toBeTruthy();
  });

  it("marks resolved and aborts through confirmation", async () => {
    const snapshot = await mockAdapter.conflictMarkResolved(
      "demo",
      1,
      "demo:conflict:0",
      "demofp:24",
      "workingFile"
    );
    expect(snapshot.repoId).toBeTruthy();
    const prepared = await mockAdapter.confirmationPrepare("demo", 1, "merge_abort", ["merge"]);
    expect(prepared.summary).toBeTruthy();
    const aborted = await mockAdapter.mergeAbort("demo", 1, prepared.confirmationToken);
    expect(aborted.repoId).toBeTruthy();
  });
});
