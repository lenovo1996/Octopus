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
