import type { DiffHunk } from "../ipc/types";

/**
 * Ordinal of a rendered hunk row among the hunk's changed lines.
 * Counts `add`/`delete` rows only (context and `noNewline` marker rows
 * never count) — the same rule the backend `select_patch_lines` uses, so
 * the ordinal the UI sends always addresses the intended patch row.
 * Returns null for rows that are not stageable single lines.
 */
export function changedLineOrdinal(hunk: DiffHunk, rowIndex: number): number | null {
  const row = hunk.lines[rowIndex];
  if (!row || (row.kind !== "add" && row.kind !== "delete")) return null;
  let ordinal = 0;
  for (let i = 0; i < rowIndex; i += 1) {
    const kind = hunk.lines[i]?.kind;
    if (kind === "add" || kind === "delete") ordinal += 1;
  }
  return ordinal;
}

/** True when a hunk allows per-line actions (no trailing-newline marker). */
export function hunkAllowsLineActions(hunk: DiffHunk): boolean {
  return !hunk.lines.some((line) => line.kind === "noNewline");
}
