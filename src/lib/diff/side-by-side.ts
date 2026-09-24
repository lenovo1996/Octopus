// Side-by-side pairing for unified hunk lines. Deletes buffered ahead of
// adds pair left/right; anything unpaired keeps its own side; context and
// no-newline markers span both columns.
import type { DiffLine } from "../ipc/types";

export type SideKind = "both" | "left" | "right";

export interface SideRow {
  kind: SideKind;
  left: DiffLine | null;
  right: DiffLine | null;
}

export function pairHunkLines(lines: DiffLine[]): SideRow[] {
  const rows: SideRow[] = [];
  let pending: DiffLine[] = [];
  const flushDeletes = () => {
    for (const line of pending) rows.push({ kind: "left", left: line, right: null });
    pending = [];
  };
  for (const line of lines) {
    if (line.kind === "context" || line.kind === "noNewline") {
      flushDeletes();
      rows.push({ kind: "both", left: line, right: line });
    } else if (line.kind === "delete") {
      pending.push(line);
    } else {
      const left = pending.shift() ?? null;
      if (left) rows.push({ kind: "both", left, right: line });
      else rows.push({ kind: "right", left: null, right: line });
    }
  }
  flushDeletes();
  return rows;
}
