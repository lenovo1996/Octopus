<script lang="ts">
  import { onMount } from "svelte";
  let { summary, busy, error, onConfirm, onCancel }: {
    summary: string;
    busy: boolean;
    error: string | null;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
  let cancelButton: HTMLButtonElement;
  onMount(() => cancelButton.focus());

  function keyDown(event: KeyboardEvent): void {
    if (!event.defaultPrevented && event.key === "Escape" && !busy) onCancel();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-confirm" role="dialog" aria-modal="true" aria-labelledby="discard-title">
    <div class="gd-danger-mark" aria-hidden="true">↶</div>
    <div class="gd-copy">
      <span class="gd-eyebrow">Destructive action</span>
      <h2 id="discard-title">Discard working changes?</h2>
      <p>{summary}</p>
      {#if error}<p class="gd-error" role="alert">{error}</p>{/if}
    </div>
    <footer>
      <button bind:this={cancelButton} type="button" class="gd-cancel" onclick={onCancel} disabled={busy}>Cancel</button>
      <button type="button" class="gd-danger" onclick={onConfirm} disabled={busy}>
        {busy ? "Discarding…" : "Discard changes"}
      </button>
    </footer>
  </div>
</div>

<style>
  .gd-modal-backdrop { position: fixed; inset: 0; display: grid; place-items: center; padding: 24px; background: rgba(5, 7, 10, .72); z-index: 40; }
  .gd-confirm { display: grid; grid-template-columns: 42px 1fr; gap: 14px; width: min(460px, calc(100vw - 48px)); padding: 22px; background: var(--gd-panel); border: 1px solid color-mix(in srgb, var(--gd-danger) 45%, var(--gd-border)); border-radius: var(--gd-radius-panel); box-shadow: 0 18px 60px rgba(0, 0, 0, .38); }
  .gd-danger-mark { display: grid; place-items: center; width: 38px; height: 38px; color: var(--gd-danger); background: color-mix(in srgb, var(--gd-danger) 12%, transparent); border: 1px solid color-mix(in srgb, var(--gd-danger) 32%, transparent); border-radius: 50%; font-size: 21px; }
  .gd-copy { min-width: 0; }
  .gd-eyebrow { color: var(--gd-danger); font-size: 10px; font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
  h2 { margin: 4px 0 9px; font-size: 17px; font-weight: 620; }
  p { margin: 0; color: var(--gd-text-secondary); font-size: 12px; line-height: 1.6; overflow-wrap: anywhere; }
  .gd-error { margin-top: 10px; padding: 8px 10px; color: var(--gd-danger); background: color-mix(in srgb, var(--gd-danger) 8%, transparent); border-left: 2px solid var(--gd-danger); }
  footer { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
  button { min-width: 108px; padding: 8px 13px; color: var(--gd-text); border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); cursor: pointer; font: 600 12px var(--gd-font-ui); }
  .gd-cancel { background: transparent; }
  .gd-cancel:hover { background: var(--gd-surface-hover); }
  .gd-danger { color: white; background: var(--gd-danger); border-color: var(--gd-danger); }
  .gd-danger:hover { filter: brightness(1.08); }
  button:disabled { opacity: .55; cursor: not-allowed; }
  button:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
