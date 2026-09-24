import { describe, expect, it } from "vitest";
import { asSyncKind, syncFailureMessage } from "../../src/lib/sync/errors";

describe("sync failure presentation", () => {
  it("gives HTTPS credential-helper recovery without exposing the remote", () => {
    const message = syncFailureMessage("push", "AUTH_REQUIRED", null, "https://example.com/private/repo.git");
    expect(message).toContain("Git credential helper");
    expect(message).toContain("repository access");
    expect(message).not.toContain("example.com");
  });

  it("gives SSH-agent recovery for URL and scp-style remotes", () => {
    expect(syncFailureMessage("push", "AUTH_REQUIRED", null, "ssh://git@example.com/repo.git")).toContain("SSH agent");
    expect(syncFailureMessage("fetch", "AUTH_REQUIRED", null, "git@example.com:repo.git")).toContain("SSH agent");
  });

  it("uses the structured safe message for other failures", () => {
    const error = { code: "GIT_ERROR" as const, message: "Safe failure", recovery: "refresh" as const, retryable: true };
    expect(syncFailureMessage("push", "GIT_ERROR", error, null)).toBe("Safe failure");
    expect(asSyncKind("push")).toBe("push");
    expect(asSyncKind("clone")).toBeNull();
  });
});
