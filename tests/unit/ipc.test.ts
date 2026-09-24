import { describe, expect, it } from "vitest";
import { normalizeTransportError, unwrapResult } from "../../src/lib/ipc/client";
import { demoPreflight } from "../../src/mocks/demoPreflight";
import type { ApiResult, ErrorCode, PreflightData } from "../../src/lib/ipc/types";

const REQUEST_ID = "req-unit-001";

function okEnvelope(data: PreflightData): ApiResult<PreflightData> {
  return { ok: true, data, requestId: REQUEST_ID };
}

describe("ApiResult envelope (IPC contract v1)", () => {
  it("round-trips the success variant through JSON with camelCase keys", () => {
    const parsed = JSON.parse(JSON.stringify(okEnvelope(demoPreflight))) as ApiResult<PreflightData>;
    expect(unwrapResult<PreflightData>(parsed, REQUEST_ID)).toEqual(demoPreflight);
    expect(JSON.stringify(okEnvelope(demoPreflight))).toContain('"requestId"');
  });

  it("throws the enclosed AppError for the failure variant", () => {
    const raw: ApiResult<PreflightData> = {
      ok: false,
      error: { code: "GIT_NOT_FOUND", message: "Git not found", recovery: "configureGit", retryable: false },
      requestId: REQUEST_ID
    };
    const parsed = JSON.parse(JSON.stringify(raw)) as ApiResult<PreflightData>;
    try {
      unwrapResult<PreflightData>(parsed, REQUEST_ID);
      expect.unreachable("expected AppError to be thrown");
    } catch (error) {
      expect(error).toMatchObject({ code: "GIT_NOT_FOUND", recovery: "configureGit" });
    }
  });

  it("rejects envelopes that belong to a different request", () => {
    expect(() => unwrapResult(okEnvelope(demoPreflight), "req-other")).toThrowError(/Mismatched response/);
  });

  it("rejects malformed envelopes instead of inventing Git failures", () => {
    for (const raw of [null, 42, "ok", {}, { ok: true, requestId: REQUEST_ID }]) {
      expect(() => unwrapResult(raw, REQUEST_ID)).toThrowError(/Invalid response|Mismatched response/);
    }
  });

  it("normalizes transport rejections to IO_ERROR, never a Git failure", () => {
    const error = normalizeTransportError(new Error("window closed"), REQUEST_ID);
    expect(error.code).toBe("IO_ERROR");
    expect(error.retryable).toBe(false);
  });

  it("explains Tauri string deserialization failures without exposing raw payloads", () => {
    const error = normalizeTransportError('invalid args `request` for command `diff_read`: unknown variant `worktree` private-secret', REQUEST_ID);
    expect(error.message).toContain("Native request format was rejected");
    expect(error.message).not.toContain("private-secret");
    expect(normalizeTransportError(new Error("https://user:secret@example.com/repo"), REQUEST_ID).message).not.toContain("secret");
  });

  it("covers every contract error code", () => {
    const codes: ErrorCode[] = [
      "GIT_NOT_FOUND", "GIT_VERSION_UNSUPPORTED", "NOT_REPOSITORY", "BARE_REPOSITORY",
      "REPO_UNAVAILABLE", "INVALID_ARGUMENT", "PATH_INVALID", "REF_INVALID", "TRUST_REQUIRED",
      "REPO_BUSY", "REPO_LOCKED", "STALE_STATE", "DIRTY_WORKTREE", "EMPTY_INDEX",
      "IDENTITY_MISSING", "CONFLICTS_PRESENT", "DIVERGED", "AUTH_REQUIRED", "NETWORK_ERROR",
      "HOOK_FAILED", "SIGNING_FAILED", "OUTPUT_LIMIT", "UNSUPPORTED", "CANCELLED", "TIMEOUT",
      "IO_ERROR", "GIT_ERROR"
    ];
    expect(codes).toHaveLength(27);
    for (const code of codes) {
      const raw: ApiResult<PreflightData> = {
        ok: false,
        error: { code, message: code, recovery: "none", retryable: false },
        requestId: REQUEST_ID
      };
      try {
        unwrapResult(JSON.parse(JSON.stringify(raw)) as ApiResult<PreflightData>, REQUEST_ID);
        expect.unreachable(`expected ${code} to be thrown`);
      } catch (error) {
        expect(error).toMatchObject({ code });
      }
    }
  });
});
