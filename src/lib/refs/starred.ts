// Starred (pinned) branches for the sidebar Local/Remote panels. Stars are
// a local UI preference: persisted in localStorage per repository scope
// (stable worktree key), never sent to Git or the backend.
import type { RefItem } from "../ipc/types";

export const STARRED_KEY = "gitdock.starred.v1";

export interface StarStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function realStorage(): StarStorage | null {
  try {
    if (typeof localStorage === "undefined") return null;
    return localStorage;
  } catch {
    return null;
  }
}

/** Starred ref ids for one scope; corrupt or missing data reads as empty. */
export function loadStarred(
  scope: string,
  storage: StarStorage | null = realStorage()
): string[] {
  if (!storage) return [];
  try {
    const raw = storage.getItem(STARRED_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return [];
    const ids = (parsed as Record<string, unknown>)[scope];
    if (!Array.isArray(ids)) return [];
    return ids.filter((id): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

export function saveStarred(
  scope: string,
  ids: string[],
  storage: StarStorage | null = realStorage()
): void {
  if (!storage) return;
  try {
    const raw = storage.getItem(STARRED_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    const all =
      typeof parsed === "object" && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {};
    all[scope] = [...ids];
    storage.setItem(STARRED_KEY, JSON.stringify(all));
  } catch {
    // Star loss beats a crash; the in-memory set still applies this session.
  }
}

/** Starred refs float to the top, otherwise keeping backend order (stable). */
export function sortStarred(
  refs: RefItem[],
  starred: Set<string>
): RefItem[] {
  return [...refs].sort((a, b) => {
    const sa = starred.has(a.refId) ? 0 : 1;
    const sb = starred.has(b.refId) ? 0 : 1;
    return sa - sb;
  });
}
