import type { ChangedFile } from "../ipc/types";

/** Side of the Working changes inspector holding the open diff. */
export type OpenDiffSide = "worktree" | "index";

/**
 * Re-resolve an open working-copy diff against a fresh status listing.
 *
 * Every listing issues fresh path tokens, so the open diff's `pathId` goes
 * stale on each reload (including fetch-triggered ones, which never touch
 * working-copy content). The row is matched by display path on the same
 * side instead; when the file genuinely left that side there is nothing
 * left to show and the caller closes the view. Mirrors the Staged/Unstaged
 * group visibility: blank or ignored statuses never match.
 */
export function resolveOpenDiffRow(
  files: ChangedFile[] | null,
  displayPath: string,
  side: OpenDiffSide
): ChangedFile | null {
  if (!files) return null;
  const status = side === "worktree" ? "worktreeStatus" : "indexStatus";
  return (
    files.find((file) => file.displayPath === displayPath && file[status] !== " " && file[status] !== "!") ??
    null
  );
}
