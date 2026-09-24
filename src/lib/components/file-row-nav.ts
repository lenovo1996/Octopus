// Keyboard navigation across Working Changes file rows (Unstaged + Staged
// in visual order). ArrowUp/ArrowDown move one row, Home/End jump to the
// ends; anything else (or no valid move) returns null and keeps focus.
export function neighborRowIndex(
  rowCount: number,
  current: number,
  key: string
): number | null {
  if (rowCount <= 0 || current < 0 || current >= rowCount) return null;
  switch (key) {
    case "ArrowDown":
      return current + 1 < rowCount ? current + 1 : null;
    case "ArrowUp":
      return current - 1 >= 0 ? current - 1 : null;
    case "Home":
      return current === 0 ? null : 0;
    case "End":
      return current === rowCount - 1 ? null : rowCount - 1;
    default:
      return null;
  }
}
