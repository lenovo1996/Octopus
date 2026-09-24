<script lang="ts">
  // Merge start dialog (T13): pick a source ref, confirm the target HEAD,
  // then merge with --no-ff --no-commit for a review stop.
  import type { AppError, RefItem } from "../ipc/types";

  interface Props {
    sources: RefItem[];
    targetLabel: string;
    targetOidShort: string | null;
    selectedSource: string;
    busy: boolean;
    error: AppError | string | null;
    canStart: boolean;
    startHint: string;
    onSelectSource: (refId: string) => void;
    onStart: () => void;
    onClose: () => void;
  }

  let {
    sources,
    targetLabel,
    targetOidShort,
    selectedSource,
    busy,
    error,
    canStart,
    startHint,
    onSelectSource,
    onStart,
    onClose
  }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  function errorText(e: AppError | string): string {
    return typeof e === "string" ? e : `${e.code}: ${e.message}`;
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Merge">
    <h2>Merge</h2>
    {#if error}
      <p class="gd-error" role="alert">{errorText(error)}</p>
    {/if}
    <p class="gd-muted">Merge into <strong>{targetLabel}</strong>{#if targetOidShort} ({targetOidShort}){/if} with a review stop (never fast-forwards silently).</p>
    <label class="gd-field">
      <span>Source branch or ref</span>
      <select value={selectedSource} onchange={(e) => onSelectSource(e.currentTarget.value)} aria-label="Merge source">
        {#each sources as source (source.refId)}
          <option value={source.refId}>{source.label} ({source.oid.slice(0, 8)})</option>
        {/each}
      </select>
    </label>
    {#if sources.length === 0}
      <p class="gd-muted">No other branch to merge from.</p>
    {/if}
    <div class="gd-modal-foot">
      <button type="button" onclick={onClose}>Cancel</button>
      <button type="button" class="gd-primary" disabled={!canStart || busy} title={startHint} onclick={onStart}>
        {busy ? "Starting…" : "Start merge"}
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
    min-width: 420px;
    max-width: 560px;
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
    gap: 4px;
    margin: var(--gd-space-2) 0;
    font-size: var(--gd-font-size-small);
  }
  .gd-field select {
    background: var(--gd-background);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    color: var(--gd-text);
    padding: 6px 8px;
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
