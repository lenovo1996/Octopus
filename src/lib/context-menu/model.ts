export interface ContextMenuItem {
  id: string;
  label: string;
  hint?: string;
  title?: string;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
  action: () => void | Promise<void>;
}

export interface MenuPosition {
  left: number;
  top: number;
}

const VIEWPORT_MARGIN = 8;

export function clampMenuPosition(
  x: number,
  y: number,
  menuWidth: number,
  menuHeight: number,
  viewportWidth: number,
  viewportHeight: number
): MenuPosition {
  return {
    left: Math.max(VIEWPORT_MARGIN, Math.min(x, viewportWidth - menuWidth - VIEWPORT_MARGIN)),
    top: Math.max(VIEWPORT_MARGIN, Math.min(y, viewportHeight - menuHeight - VIEWPORT_MARGIN))
  };
}

export function firstEnabledIndex(items: Pick<ContextMenuItem, "disabled">[]): number {
  return items.findIndex((item) => !item.disabled);
}

export function nextEnabledIndex(
  items: Pick<ContextMenuItem, "disabled">[],
  current: number,
  direction: -1 | 1
): number {
  if (items.length === 0) return -1;
  for (let offset = 1; offset <= items.length; offset += 1) {
    const index = (current + direction * offset + items.length) % items.length;
    if (!items[index].disabled) return index;
  }
  return -1;
}

export function isContextMenuKey(event: Pick<KeyboardEvent, "key" | "shiftKey">): boolean {
  return event.key === "ContextMenu" || (event.shiftKey && event.key === "F10");
}

export function pointFromContextEvent(event: MouseEvent | KeyboardEvent): MenuPosition {
  if (event instanceof MouseEvent) return { left:event.clientX, top:event.clientY };
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  return { left:rect.left + Math.min(24, rect.width / 2), top:rect.bottom - 2 };
}
