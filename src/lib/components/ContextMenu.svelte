<script lang="ts">
  import { tick } from "svelte";
  import {
    clampMenuPosition,
    firstEnabledIndex,
    nextEnabledIndex,
    type ContextMenuItem
  } from "../context-menu/model";

  let { x, y, items, label = "Context menu", onClose }: {
    x: number;
    y: number;
    items: ContextMenuItem[];
    label?: string;
    onClose: () => void;
  } = $props();

  let menu: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);
  let activeIndex = $state(-1);

  $effect(() => {
    const menuX = x;
    const menuY = y;
    const currentItems = items;
    activeIndex = firstEnabledIndex(currentItems);
    void tick().then(() => {
      if (!menu) return;
      const rect = menu.getBoundingClientRect();
      const position = clampMenuPosition(menuX, menuY, rect.width, rect.height, window.innerWidth, window.innerHeight);
      left = position.left;
      top = position.top;
      void tick().then(() => menu?.querySelector<HTMLElement>(`[data-menu-index="${activeIndex}"]`)?.focus());
    });
  });

  function windowPointerDown(event: PointerEvent): void {
    if (menu && !menu.contains(event.target as Node)) onClose();
  }

  function run(item: ContextMenuItem): void {
    if (item.disabled) return;
    onClose();
    try { void Promise.resolve(item.action()).catch(() => {}); }
    catch { /* The owning panel keeps operation and clipboard errors in its persistent UI. */ }
  }

  function keyDown(event: KeyboardEvent): void {
    let next = activeIndex;
    if (event.key === "ArrowDown") next = nextEnabledIndex(items, activeIndex, 1);
    else if (event.key === "ArrowUp") next = nextEnabledIndex(items, activeIndex, -1);
    else if (event.key === "Home") next = firstEnabledIndex(items);
    else if (event.key === "End") next = nextEnabledIndex(items, 0, -1);
    else if (event.key === "Escape" || event.key === "Tab") { onClose(); return; }
    else if ((event.key === "Enter" || event.key === " ") && activeIndex >= 0) {
      event.preventDefault();
      run(items[activeIndex]);
      return;
    } else return;
    event.preventDefault();
    event.stopPropagation();
    activeIndex = next;
    menu?.querySelector<HTMLElement>(`[data-menu-index="${activeIndex}"]`)?.focus();
  }
</script>

<svelte:window onpointerdown={windowPointerDown} onblur={onClose} onresize={onClose} />

<div class="gd-context-menu" role="menu" aria-label={label} tabindex="-1" bind:this={menu}
  style={`left: ${left}px; top: ${top}px`} onkeydown={keyDown}>
  {#each items as item, index (item.id)}
    {#if item.separatorBefore}<div class="gd-context-separator" role="separator"></div>{/if}
    <button type="button" role="menuitem" data-menu-index={index} tabindex={index === activeIndex ? 0 : -1}
      disabled={item.disabled} class:danger={item.danger} class:active={index === activeIndex}
      title={item.title}
      aria-label={item.disabled && item.title ? `${item.label}. Planned. ${item.title}` : item.label}
      onmouseenter={() => { if (!item.disabled) activeIndex = index; }} onclick={() => run(item)}>
      <span>{item.label}</span>{#if item.hint}<kbd>{item.hint}</kbd>{/if}
    </button>
  {/each}
</div>

<style>
  .gd-context-menu {
    position: fixed;
    z-index: 60;
    min-width: 210px;
    max-width: min(320px, calc(100vw - 16px));
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 5px;
    border: 1px solid color-mix(in srgb, var(--gd-border) 82%, var(--gd-text-secondary));
    border-radius: 6px;
    background: color-mix(in srgb, var(--gd-panel) 96%, var(--gd-surface-raised));
    box-shadow: 0 14px 36px color-mix(in srgb, var(--gd-canvas) 78%, transparent), inset 0 1px color-mix(in srgb, var(--gd-text) 5%, transparent);
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    width: 100%;
    min-height: 30px;
    padding: 5px 9px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--gd-text);
    font: 12px var(--gd-font-ui);
    text-align: left;
    cursor: default;
  }
  button:hover:not(:disabled), button.active:not(:disabled) { background: var(--gd-surface-hover); }
  button:active:not(:disabled) { transform: translateY(1px); }
  button:focus { outline: none; }
  button:focus-visible { box-shadow: inset 0 0 0 2px var(--gd-focus); }
  button:disabled { color: color-mix(in srgb, var(--gd-text-secondary) 42%, transparent); }
  button.danger:not(:disabled) { color: var(--gd-danger); }
  button.danger:disabled { color: color-mix(in srgb, var(--gd-danger) 56%, transparent); }
  kbd { color: var(--gd-text-secondary); font: 10px var(--gd-font-code); white-space: nowrap; }
  .gd-context-separator { height: 1px; margin: 5px 4px; background: var(--gd-border); }
</style>
