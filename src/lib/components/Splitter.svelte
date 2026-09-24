<script lang="ts">
  // Draggable + keyboard-accessible pane splitter (8px hit area, 1px separator).
  interface Props {
    onResize: (delta: number) => void;
    onReset: () => void;
    label: string;
  }

  let { onResize, onReset, label }: Props = $props();
  let dragging = $state(false);
  let previousX = 0;

  function pointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    previousX = e.clientX;
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function pointerMove(e: PointerEvent): void {
    if (!dragging) return;
    const delta = e.clientX - previousX;
    previousX = e.clientX;
    onResize(delta);
  }

  function pointerUp(): void {
    dragging = false;
  }

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      onResize(-8);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      onResize(8);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: UI spec §2 requires keyboard-resizable separator with Left/Right + focus -->
<div
  class="gd-splitter"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  tabindex="0"
  onpointerdown={pointerDown}
  onpointermove={pointerMove}
  onpointerup={pointerUp}
  onpointercancel={pointerUp}
  onlostpointercapture={pointerUp}
  onkeydown={keyDown}
  ondblclick={onReset}
></div>

<style>
  .gd-splitter {
    flex: 0 0 8px;
    margin: 0 -3.5px;
    cursor: col-resize;
    touch-action: none;
    position: relative;
    z-index: 5;
  }
  .gd-splitter::after {
    content: "";
    position: absolute;
    inset: 0 3.5px;
    background: var(--gd-border);
  }
  .gd-splitter:hover::after,
  .gd-splitter:focus-visible::after,
  .gd-splitter.dragging::after {
    background: var(--gd-accent);
  }
  .gd-splitter:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: -2px;
  }
</style>
