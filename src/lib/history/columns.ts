export type HistoryColumn = "branches" | "graph" | "subject" | "author";
export type ColumnWidths = Record<HistoryColumn, number | null>;
export const COLUMN_LIMITS = {
  branches: { min: 110, max: 420, initial: 150 },
  graph: { min: 64, max: 1600, initial: 84 },
  subject: { min: 200, max: 1400, initial: 320 },
  author: { min: 100, max: 420, initial: 140 }
} as const;
export const COLUMNS_KEY = "gitdock.history-columns.v1";

export function defaultColumns(): ColumnWidths {
  return { branches: 150, graph: 84, subject: null, author: 140 };
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
