import type { RefItem } from "../ipc/types";

export interface RefBadge { id: string; name: string; source: string; kind: "local" | "remote" | "tag"; fullName: string }
export function refBadge(ref: RefItem): RefBadge {
  const kind = ref.kind;
  if (kind === "remote") {
    const label = ref.fullName.replace(/^refs\/remotes\//, "");
    const slash = label.indexOf("/");
    return { id: ref.refId, name: slash < 0 ? label : label.slice(slash + 1), source: slash < 0 ? "remote" : label.slice(0, slash), kind, fullName: ref.fullName };
  }
  return { id: ref.refId, name: ref.label, source: kind === "tag" ? "tag" : "local", kind: kind === "tag" ? "tag" : "local", fullName: ref.fullName };
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
