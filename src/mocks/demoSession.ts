import type { HeadState, RepoSnapshot } from "../lib/ipc/types";
import { demoCommits } from "./demoRepo";

/** Demo session shaped like a real `repo_open` result (browser mock only). */
export const demoSession: RepoSnapshot = {
  repoId: "demo-session",
  workspaceKey: "demo-worktree:/demo/gitdock-demo",
  version: 1,
  displayName: "gitdock-demo",
  displayPath: "/demo/gitdock-demo",
  head: { kind: "branch", refId: "refs/heads/main", name: "main", oid: demoCommits[0].oid },
  trust: "trusted",
  state: "normal",
  mergeOrigin: null,
  upstream: null,
  lastFetchAt: null,
  activeOperation: null,
  stagedCount: null,
  unstagedCount: null,
  conflictCount: null
};

export const demoRecents = [
  { entryId: "/demo/gitdock-demo", key: "/demo/gitdock-demo", displayPath: "/demo/gitdock-demo", lastOpenedAt: 1790000000 },
  { entryId: "/demo/website", key: "/demo/website", displayPath: "/demo/website", lastOpenedAt: 1789900000 }
];

/** Short human label for each HEAD kind (unborn/detached called out). */
export function headLabel(head: HeadState): string {
  switch (head.kind) {
    case "branch":
      return head.name;
    case "detached":
      return `detached ${head.oid.slice(0, 8)}`;
    case "unborn":
      return `unborn ${head.name}`;
  }
}
