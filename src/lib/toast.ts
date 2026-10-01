import { writable } from "svelte/store";

export type ToastKind = "success" | "info";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

/** Maximum visible toasts; older ones drop off when exceeded. */
export const MAX_TOASTS = 4;
/** Auto-dismiss delay for every toast. */
export const TOAST_TTL_MS = 4000;

let nextId = 1;
export const toasts = writable<ToastItem[]>([]);

/** Show a toast that dismisses itself after `ttlMs` (0 keeps it). */
export function pushToast(kind: ToastKind, message: string, ttlMs = TOAST_TTL_MS): number {
  const id = nextId++;
  toasts.update((items) => [...items.slice(-(MAX_TOASTS - 1)), { id, kind, message }]);
  if (ttlMs > 0) {
    setTimeout(() => dismissToast(id), ttlMs);
  }
  return id;
}

export function dismissToast(id: number): void {
  toasts.update((items) => items.filter((toast) => toast.id !== id));
}

/** Test-only reset for deterministic ids. */
export function resetToastsForTests(): void {
  nextId = 1;
  toasts.set([]);
}
