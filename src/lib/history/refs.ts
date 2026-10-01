import type { RefItem } from "../ipc/types";

export interface RefBadge { id: string; name: string; source: string; kind: "local" | "remote" | "tag"; fullName: string; current: boolean }
export function refBadge(ref: RefItem): RefBadge {
  const kind = ref.kind;
  if (kind === "remote") {
    const label = ref.fullName.replace(/^refs\/remotes\//, "");
    const slash = label.indexOf("/");
    return { id: ref.refId, name: slash < 0 ? label : label.slice(slash + 1), source: slash < 0 ? "remote" : label.slice(0, slash), kind, fullName: ref.fullName, current: ref.current };
  }
  return { id: ref.refId, name: ref.label, source: kind === "tag" ? "tag" : "local", kind: kind === "tag" ? "tag" : "local", fullName: ref.fullName, current: ref.current };
}

/**
 * Resolve a rendered graph badge back to its RefItem. Badge ids are the
 * ref's refId, so the sidebar branch menu (including create-pr) can run
 * directly from a graph badge. Null when the badge is stale.
 */
export function refItemForBadge(refs: RefItem[], badge: Pick<RefBadge, "id">): RefItem | null {
  return refs.find((ref) => ref.refId === badge.id) ?? null;
}

/**
 * The one badge a graph row shows when a commit carries several refs.
 * The last successful checkout choice wins, then the current branch,
 * locals, origin, other remotes, and tags; the rest collapse behind a
 * "+N" suffix with the full list on hover.
 */
export function primaryBadge(badges: RefBadge[], preferredRefId: string | null = null): RefBadge | null {
  if (badges.length === 0) return null;
  return (
    badges.find((b) => b.id === preferredRefId) ??
    badges.find((b) => b.current) ??
    badges.find((b) => b.kind === "local") ??
    badges.find((b) => b.kind === "remote" && b.source === "origin") ??
    badges.find((b) => b.kind === "remote") ??
    badges[0]
  );
}

/** Keep the remote name visible when local and remote twins share a commit. */
export function badgeLabel(badge: RefBadge): string {
  return badge.kind === "remote" ? badge.fullName.replace(/^refs\/remotes\//, "") : badge.name;
}

export interface TipRect {
  left: number;
  top: number;
  bottom: number;
}

export interface TipPlacement {
  x: number;
  y: number;
  above: boolean;
}

/**
 * Where the "+N" hover panel opens: below the pill with a 4px gap,
 * clamped inside the viewport, flipping above near the bottom edge.
 */
export function branchTipPlacement(rect: TipRect, viewportWidth: number, viewportHeight: number): TipPlacement {
  const gap = 4;
  const maxWidth = 280;
  const maxHeight = 200;
  const x = Math.max(8, Math.min(rect.left, viewportWidth - maxWidth - 8));
  if (rect.bottom + gap + maxHeight > viewportHeight && rect.top - gap > maxHeight) {
    return { x, y: rect.top - gap, above: true };
  }
  return { x, y: rect.bottom + gap, above: false };
}

export function refsByCommit(refs: RefItem[]): Map<string, RefBadge[]> {
  const result = new Map<string, RefBadge[]>();
  for (const ref of refs) {
    const badges = result.get(ref.oid) ?? [];
    badges.push(refBadge(ref));
    result.set(ref.oid, badges);
  }
  for (const badges of result.values()) badges.sort((a, b) => ({local:0,remote:1,tag:2}[a.kind] - {local:0,remote:1,tag:2}[b.kind]) || a.name.localeCompare(b.name));
  return result;
}
