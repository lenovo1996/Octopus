<script lang="ts">
  // Settings dialog (T14): validated font scale with optimistic version.
  import type { AppError } from "../ipc/types";

  interface Props {
    fontScale: number;
    version: number | null;
    busy: boolean;
    error: AppError | null;
    onScale: (value: number) => void;
    onSave: () => void;
    onClose: () => void;
  }

  let { fontScale, version, busy, error, onScale, onSave, onClose }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  const percent = $derived(Math.round(fontScale * 100));
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Settings">
    <h2>Settings</h2>
    {#if error}
      <p class="gd-error" role="alert">{error.code}: {error.message}</p>
    {/if}
    <label class="gd-field">
      <span>Interface font size ({percent}%)</span>
      <input
        type="range"
        min={87.5}
        max={125}
        step={2.5}
        value={fontScale * 100}
        oninput={(e) => onScale(Number(e.currentTarget.value) / 100)}
        aria-label="Interface font size"
      />
    </label>
    <p class="gd-muted">Applies immediately and persists across restarts{version === null ? "" : ` (settings v${version})`}.</p>
    <div class="gd-modal-foot">
      <button type="button" onclick={onClose}>Close</button>
      <button type="button" class="gd-primary" disabled={busy} onclick={onSave}>
        {busy ? "Saving…" : "Save"}
      </button>
    </div>
  </div>
</div>

<style>
  .gd-modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }
  .gd-modal {
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-4);
    min-width: 380px;
    max-width: 480px;
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-3);
    font-size: 15px;
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-field {
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-2);
    margin: var(--gd-space-2) 0;
    font-size: var(--gd-font-size-small);
  }
  .gd-modal-foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-3);
  }
  .gd-primary {
    padding: 6px 14px;
    cursor: pointer;
  }
</style>
