import type { ChangedFile } from "../ipc/types";
import { partitionStatus } from "./partition";

export interface DiscardAllTarget {
  pathId: string;
  displayPath: string;
  /** True for untracked (`?`) files: discarding deletes the file. */
  untracked: boolean;
}

/**
 * Collect the unstaged, non-conflicted rows that "Discard all" acts on.
 * Staged-only rows are untouched (per-file discard restores the unstaged
 * side only); conflicted rows stay in the conflict flow.
 */
export function collectDiscardAllTargets(files: ChangedFile[] | null): DiscardAllTarget[] {
  if (!files) return [];
  const { unstaged } = partitionStatus(files.filter((f) => !f.conflicted));
  return unstaged.map((f) => ({
    pathId: f.pathId,
    displayPath: f.displayPath,
    untracked: f.worktreeStatus === "?"
  }));
}

/** Single-confirmation copy for the Discard-all modal. */
export function discardAllSummary(targets: DiscardAllTarget[]): string {
  const total = targets.length;
  if (total === 0) return "No unstaged changes to discard.";
  const untracked = targets.filter((t) => t.untracked).length;
  const tracked = total - untracked;
  const parts: string[] = [];
  if (tracked > 0)
    parts.push(
      `${tracked} tracked ${tracked === 1 ? "file restores its index version" : "files restore their index versions"}`
    );
  if (untracked > 0)
    parts.push(
      `${untracked} untracked ${untracked === 1 ? "file is deleted" : "files are deleted"}`
    );
  return (
    `Discard all ${total} unstaged ${total === 1 ? "change" : "changes"} (${parts.join("; ")})? ` +
    `Staged changes are kept. This cannot be undone by GitDock.`
  );
}
