export type HistoryColumn = "branches" | "graph" | "subject" | "author" | "date";
export type ColumnWidths = Record<HistoryColumn, number | null>;
export const COLUMN_LIMITS = {
  branches: { min: 110, max: 420, initial: 150 },
  graph: { min: 64, max: 1600, initial: 84 },
  subject: { min: 200, max: 1400, initial: 320 },
  author: { min: 100, max: 420, initial: 140 },
  date: { min: 90, max: 300, initial: 120 }
} as const;
export const COLUMNS_KEY = "gitdock.history-columns.v1";

export function defaultColumns(): ColumnWidths {
  return { branches: 150, graph: 84, subject: null, author: 140, date: 120 };
}

/**
 * Compact commit timestamp for the Date column ("3h", "2d", "24 Sep").
 * Falls back to the date part when unparsable; full value stays in title.
 */
export function formatCommitDate(iso: string, now: number = Date.now()): string {
  const time = Date.parse(iso);
  if (!Number.isFinite(time)) return iso.slice(0, 10);
  const diffMs = now - time;
  if (diffMs < 0) return iso.slice(0, 10);
  const minutes = Math.floor(diffMs / 60000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const date = new Date(time);
  const day = date.getUTCDate();
  const month = date.toLocaleString("en-US", { month: "short", timeZone: "UTC" });
  const year = date.getUTCFullYear();
  const thisYear = new Date(now).getUTCFullYear();
  return year === thisYear ? `${day} ${month}` : `${day} ${month} ${year}`;
}

export function clampColumn(column: HistoryColumn, width: number): number {
  const limits = COLUMN_LIMITS[column];
  return Number.isFinite(width) ? Math.max(limits.min, Math.min(limits.max, Math.round(width))) : limits.initial;
}

export function restoreColumns(raw: string | null): ColumnWidths {
  const result = defaultColumns();
  try {
    const saved: unknown = JSON.parse(raw ?? "{}");
    if (!saved || typeof saved !== "object") return result;
    for (const column of Object.keys(result) as HistoryColumn[]) {
      const value = (saved as Record<string, unknown>)[column];
      if (typeof value === "number" && Number.isFinite(value)) result[column] = clampColumn(column, value);
    }
  } catch { /* Corrupt UI preferences fall back to defaults. */ }
  return result;
}
