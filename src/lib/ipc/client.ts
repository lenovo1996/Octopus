import { invoke } from "@tauri-apps/api/core";
import type { ApiResult, AppError, ErrorCode, RecoveryAction, RequestId } from "./types";

export type { AppError };

/** True only inside the Tauri native webview. Browser preview must use the mock adapter. */
export function isNative(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
}

function transportError(code: ErrorCode, message: string, recovery: RecoveryAction): AppError {
  return { code, message, recovery, retryable: false };
}

/**
 * Narrow a raw IPC envelope. Throws the enclosed AppError on `{ok:false}`,
 * and a transport-level AppError when the envelope is malformed or belongs
 * to a different request. Tauri transport rejections never become Git failures.
 */
export function unwrapResult<T>(raw: unknown, expectedRequestId: RequestId): T {
  if (typeof raw !== "object" || raw === null) {
    throw transportError("IO_ERROR", "Invalid response from native layer.", "retryRead");
  }
  const envelope = raw as Partial<ApiResult<T>>;
  if (envelope.requestId !== expectedRequestId) {
    throw transportError("IO_ERROR", "Mismatched response for this request.", "retryRead");
  }
  if (envelope.ok === true) {
    if (!("data" in envelope)) {
      throw transportError("IO_ERROR", "Invalid response from native layer.", "retryRead");
    }
    return (envelope as { data: T }).data;
  }
  if (envelope.ok === false && typeof envelope.error === "object" && envelope.error !== null) {
    throw envelope.error as AppError;
  }
  throw transportError("IO_ERROR", "Invalid response from native layer.", "retryRead");
}

/** Normalize a Tauri transport rejection (window closed, deserialize error, ...). */
export function normalizeTransportError(error: unknown, requestId: RequestId): AppError {
  void requestId;
  const detail = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  // Tauri rejects malformed arguments as strings. Explain the recovery without
  // exposing raw payloads, paths or credentials in arbitrary transport errors.
  const message = /invalid args|unknown variant|missing field/i.test(detail)
    ? "Native request format was rejected. Restart the updated application and try again."
    : /command .*not found/i.test(detail)
      ? "This native command is unavailable. Restart the updated application."
      : "Native call failed. Restart the application and retry.";
  return transportError("IO_ERROR", message, "retryRead");
}

export interface TypedRequest {
  requestId: RequestId;
  [key: string]: unknown;
}

/** Typed command boundary: every command goes through ApiResult<T> + requestId. */
export async function invokeCommand<T>(command: string, request: TypedRequest): Promise<T> {
  let raw: ApiResult<T>;
  try {
    raw = await invoke<ApiResult<T>>(command, { request });
  } catch (error) {
    throw normalizeTransportError(error, request.requestId);
  }
  return unwrapResult<T>(raw, request.requestId);
}

export function newRequestId(): RequestId {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  return `req-${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
}
