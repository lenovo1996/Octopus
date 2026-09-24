// Branch-list filtering for the sidebar (Local/Remote/Tags sections).
// Case-insensitive substring match on the display label; an empty query
// keeps the full list so clearing the box restores everything.
import type { RefItem } from "../ipc/types";

export function filterRefs(refs: RefItem[], query: string): RefItem[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return refs;
  return refs.filter((ref) => ref.label.toLowerCase().includes(needle));
}

/**
 * A collapsed refs section still opens while the search box filters, so
 * matching branches are never hidden behind a closed header.
 */
export function isRefsSectionOpen(collapsed: boolean, filtering: boolean): boolean {
  return !collapsed || filtering;
}

/** Suggest a local name when tracking a remote branch (`origin/foo` -> `foo`). */
export function suggestedTrackName(label: string): string {
  const rest = label.split("/").slice(1).join("/");
  return rest === "" ? label : rest;
}
