// Shell state (Svelte 5 runes) for the T02 application shell.
// UI-only selection state; Git truth always comes from IPC (T03+).

export type InspectorState = "working" | "commit" | "conflict";

export const SHELL_LIMITS = {
  sidebarMin: 180,
  sidebarMax: 300,
  sidebarDefault: 220,
  inspectorMin: 320,
  inspectorMax: 520,
  inspectorDefault: 380,
  centerMin: 420
} as const;

export function clampWidth(value: number, min: number, max: number): number {
  if (Number.isNaN(value)) return min;
  return Math.min(max, Math.max(min, value));
}

export function nextInspector(current: InspectorState): InspectorState {
  return current === "working" ? "commit" : current === "commit" ? "conflict" : "working";
}

export interface ShellState {
  inspector: InspectorState;
  selectedCommitOid: string | null;
  selectedFile: string | null;
  sidebarWidth: number;
  inspectorWidth: number;
  commitMessage: string;
}

export function initialShell(): ShellState {
  return {
    inspector: "working",
    selectedCommitOid: null,
    selectedFile: null,
    sidebarWidth: SHELL_LIMITS.sidebarDefault,
    inspectorWidth: SHELL_LIMITS.inspectorDefault,
    commitMessage: ""
  };
}
