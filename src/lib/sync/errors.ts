import type { AppError } from "../ipc/types";
import { isBitbucketCloudHttps } from "./bitbucket";

export type SyncKind = "fetch" | "pull" | "push";

function actionName(kind: string): string {
  return kind === "fetch" ? "Fetch" : kind === "pull" ? "Pull" : kind === "push" ? "Push" : "Remote operation";
}

function remoteProtocol(url: string | null): "https" | "ssh" | "other" {
  if (!url) return "other";
  const value = url.trim().toLowerCase();
  if (value.startsWith("https://") || value.startsWith("http://")) return "https";
  if (value.startsWith("ssh://") || /^[^/@\s]+@[^:\s]+:.+/.test(value)) return "ssh";
  return "other";
}

/** Builds actionable, secret-free copy for terminal remote failures. */
export function syncFailureMessage(
  kind: string,
  code: string | null,
  error: AppError | null | undefined,
  remoteUrl: string | null
): string {
  const action = actionName(kind);
  if (code === "AUTH_REQUIRED") {
    if (isBitbucketCloudHttps(remoteUrl)) {
      return `${action} authentication was rejected by Bitbucket. Connect a scoped API token with Repository Read and Write permissions, then retry.`;
    }
    const protocol = remoteProtocol(remoteUrl);
    if (protocol === "https") {
      return `${action} authentication was rejected. Refresh the access token in your Git credential helper and verify repository access, then retry.`;
    }
    if (protocol === "ssh") {
      return `${action} authentication was rejected. Start or unlock your SSH agent and verify that its key can access this repository, then retry.`;
    }
    return `${action} authentication was rejected. Authenticate with your configured credential helper or SSH agent outside Octopus, then retry.`;
  }
  if (code === "OFFLINE") return `${action} could not reach the remote. Check the network connection, then retry.`;
  if (code === "TIMEOUT") return `${action} timed out before Git received a final result. Check the connection and remote state before retrying.`;
  return error?.message ?? `${action} failed (${code ?? "GIT_ERROR"}).`;
}

export function asSyncKind(value: string): SyncKind | null {
  return value === "fetch" || value === "pull" || value === "push" ? value : null;
}
