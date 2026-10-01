import type { RecentEntry } from "../ipc/types";

/**
 * Folder name from a display path. Handles `/` and `\` separators and
 * trailing slashes; falls back to the full path when no segment survives.
 */
export function repoBaseName(displayPath: string): string {
  return displayPath.replace(/\\/g, "/").split("/").filter(Boolean).at(-1) ?? displayPath;
}

/**
 * Case-insensitive substring filter over the display path. A blank query
 * keeps every entry in order (same reference); otherwise a new array.
 */
export function filterRecentRepos(recents: RecentEntry[], query: string): RecentEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return recents;
  return recents.filter((repo) => repo.displayPath.toLowerCase().includes(needle));
}

/**
 * Wrap-around selection step for arrow-key navigation. Returns 0 when the
 * list is empty so callers never index out of range.
 */
export function movePickerSelection(index: number, delta: 1 | -1, count: number): number {
  if (count <= 0) return 0;
  return (index + delta + count) % count;
}

/** Clamp a selection into a list that may have shrunk (filter/remove). */
export function clampPickerSelection(index: number, count: number): number {
  if (count <= 0) return 0;
  return Math.min(Math.max(index, 0), count - 1);
}

/**
 * Compact relative label for a last-opened stamp (epoch seconds). Returns
 * "" for unknown or future stamps so the row simply omits the line.
 */
export function formatLastOpened(lastOpenedAt: number, nowSecs: number): string {
  const diff = Math.floor(nowSecs - lastOpenedAt);
  if (lastOpenedAt <= 0 || !Number.isFinite(diff) || diff < 0) return "";
  if (diff < 60) return "Just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  const days = Math.floor(diff / 86400);
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days}d ago`;
  const date = new Date(lastOpenedAt * 1000);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}/${month}/${day}`;
}
