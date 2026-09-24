import type { ChangedFile } from "../ipc/types";

/** Blank/marker statuses that never count as staged. Mirrors the backend counts. */
const NOT_STAGED = new Set([" ", "?", "!"]);
/** Blank/ignored statuses that never count as unstaged. */
const NOT_UNSTAGED = new Set([" ", "!"]);

export interface PartitionedStatus {
  staged: ChangedFile[];
  unstaged: ChangedFile[];
}

/**
 * Split a status listing into the Staged / Unstaged inspector sections.
 * One row may appear in both (staged then edited again); untracked `?`
 * counts as unstaged; ignored `!` appears in neither.
 */
export function partitionStatus(files: ChangedFile[]): PartitionedStatus {
  const staged = files.filter((f) => !NOT_STAGED.has(f.indexStatus));
  const unstaged = files.filter((f) => !NOT_UNSTAGED.has(f.worktreeStatus));
  return { staged, unstaged };
}

export interface WorkingSummary {
  total: number;
  added: number;
  modified: number;
  deleted: number;
}

/**
 * One-line summary of all uncommitted changes for the main-panel bar: every
 * non-conflicted file with a staged or unstaged change, counted once.
 * A file staged and edited again classifies by its worktree side.
 * Null/empty/clean/conflicted-only hides the bar.
 */
export function summarizeWorkingChanges(files: ChangedFile[] | null): WorkingSummary | null {
  if (!files) return null;
  const rows = files.filter(
    (f) =>
      !f.conflicted &&
      (!NOT_UNSTAGED.has(f.worktreeStatus) || !NOT_STAGED.has(f.indexStatus))
  );
  if (rows.length === 0) return null;
  let added = 0;
  let deleted = 0;
  for (const row of rows) {
    const side = !NOT_UNSTAGED.has(row.worktreeStatus) ? row.worktreeStatus : row.indexStatus;
    if (side === "?" || side === "A") added += 1;
    else if (side === "D") deleted += 1;
  }
  return { total: rows.length, added, deleted, modified: rows.length - added - deleted };
}
