<script lang="ts">
  let { label, width, min, max, onResize, onReset }: {
    label: string; width: number; min: number; max: number;
    onResize: (delta: number) => void; onReset: () => void;
  } = $props();
  let dragging = $state(false);
  let previousX = 0;
  function start(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    previousX = event.clientX;
    dragging = true;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }
  function move(event: PointerEvent) {
    if (!dragging) return;
    const delta = event.clientX - previousX;
    previousX = event.clientX;
    onResize(delta);
  }
  function key(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      onResize((event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 32 : 8));
    } else if (event.key === "Home") { event.preventDefault(); onReset(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: focusable separator implements the window-splitter keyboard pattern -->
<div class="gd-column-resize" class:dragging role="separator" aria-orientation="vertical"
  aria-label={`Resize ${label} column`} aria-valuenow={Math.round(width)} aria-valuemin={min} aria-valuemax={max}
  title={`Resize ${label} · drag or arrow keys · double-click to reset`} tabindex="0"
  onpointerdown={start} onpointermove={move} onpointerup={() => (dragging = false)}
  onpointercancel={() => (dragging = false)} onlostpointercapture={() => (dragging = false)}
  onkeydown={key} ondblclick={onReset}></div>

<style>
  .gd-column-resize { position: absolute; top: 0; bottom: 0; right: 0; width: 10px; z-index: 2; cursor: col-resize; touch-action: none; }
  .gd-column-resize::after { content: ""; position: absolute; top: 7px; bottom: 7px; right: 0; width: 1px; background: var(--gd-text-secondary); }
  .gd-column-resize:hover::after, .gd-column-resize.dragging::after, .gd-column-resize:focus-visible::after { background: var(--gd-accent); width: 2px; }
  .gd-column-resize:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
</style>
