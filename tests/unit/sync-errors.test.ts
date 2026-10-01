import { describe, expect, it } from "vitest";
import { asSyncKind, shouldOfferPushRecovery, syncCancelMessage, syncDoneMessage, syncFailureMessage, syncStartMessage } from "../../src/lib/sync/errors";

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

  it("labels sync toasts including force pushes", () => {
    expect(syncStartMessage("push", false)).toBe("Pushing…");
    expect(syncStartMessage("push", true)).toBe("Force pushing…");
    expect(syncStartMessage("pull", false)).toBe("Pulling…");
    expect(syncDoneMessage("push", false)).toBe("Pushed");
    expect(syncDoneMessage("push", true)).toBe("Force pushed");
    expect(syncDoneMessage("fetch", false)).toBe("Fetched");
    expect(syncCancelMessage("pull")).toBe("Pull cancelled");
  });

  it("offers push recovery only for a rejected push", () => {
    expect(shouldOfferPushRecovery("push", "DIVERGED")).toBe(true);
    expect(shouldOfferPushRecovery("push", "AUTH_REQUIRED")).toBe(false);
    expect(shouldOfferPushRecovery("pull", "DIVERGED")).toBe(false);
    expect(shouldOfferPushRecovery(null, "DIVERGED")).toBe(false);
    expect(shouldOfferPushRecovery("push", null)).toBe(false);
    expect(shouldOfferPushRecovery(undefined, undefined)).toBe(false);
  });
});
