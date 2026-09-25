import type { OpenWorkspaceEntry, RepoSnapshot } from "../ipc/types";

export interface WorkspaceState {
  snapshot: RepoSnapshot;
  busy: boolean;
  hasDraft: boolean;
  changedFiles: number | null;
  hasError: boolean;
  modalOpen: boolean;
}

export function initialWorkspace(snapshot: RepoSnapshot): WorkspaceState {
  return { snapshot, busy: false, hasDraft: false, changedFiles: null, hasError: false, modalOpen: false };
}

export function repositoryParent(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts.slice(0, -1).slice(-2).join("/") || "/";
}

/** Save payload for the open workspaces (T19): tab order preserved. */
export function openWorkspaceEntries(
  tabs: { snapshot: RepoSnapshot }[]
): OpenWorkspaceEntry[] {
  return tabs.map((tab) => ({
    key: tab.snapshot.workspaceKey,
    displayPath: tab.snapshot.displayPath
  }));
}

/** Workspace key of the active tab, or null when nothing is open. */
export function activeWorkspaceKey(
  tabs: { snapshot: RepoSnapshot }[],
  activeId: string | null
): string | null {
  return tabs.find((tab) => tab.snapshot.repoId === activeId)?.snapshot.workspaceKey ?? null;
}

/** Pick the tab to activate after a restore: saved active first, else first opened. */
export function resolveRestoredActive(
  opened: RepoSnapshot[],
  activeKey: string | null
): string | null {
  if (!opened.length) return null;
  return opened.find((snapshot) => snapshot.workspaceKey === activeKey)?.repoId ?? opened[0].repoId;
}

/**
 * Whether switching branches is worth offering an auto-stash first:
 * there are staged/unstaged changes and no conflicts (conflicted
 * worktrees cannot stash, so the backend refusal stands).
 */
export function needsStashOffer(staged: number, unstaged: number, conflicted: number): boolean {
  return staged + unstaged > 0 && conflicted === 0;
}

/** Fixed stash message so the auto-stash is recognizable in the list. */
export function autoStashMessage(targetLabel: string): string {
  return `Octopus auto-stash before switching to ${targetLabel}`;
}
