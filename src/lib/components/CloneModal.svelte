<script lang="ts">
  // Clone dialog: source URL + destination folder + explicit confirmation.
  // Clones the default branch in full; the destination must be missing or
  // empty (backend refuses otherwise and cleans up partial clones).
  import { onMount } from "svelte";
  import type { AppError } from "../ipc/types";

  interface Props {
    sourceUrl: string;
    folder: string;
    busy: boolean;
    error: AppError | null;
    onUrl: (value: string) => void;
    onBrowseFolder: () => void;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { sourceUrl, folder, busy, error, onUrl, onBrowseFolder, onConfirm, onCancel }: Props =
    $props();

  let urlInput: HTMLInputElement;
  onMount(() => {
    urlInput.focus();
  });

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onCancel();
    if (e.key === "Enter" && !busy && sourceUrl.trim() !== "" && folder !== "") onConfirm();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Clone repository">
    <h2>Clone repository</h2>
    <label>
      Repository URL
      <input
        bind:this={urlInput}
        type="text"
        value={sourceUrl}
        oninput={(e) => onUrl(e.currentTarget.value)}
        placeholder="https://github.com/owner/repo.git"
        aria-label="Repository URL"
        spellcheck="false"
      />
    </label>
    <p class="gd-muted">Destination: <code>{folder === "" ? "not chosen yet" : folder}</code></p>
    <p class="gd-muted">
      Clones the default branch into the destination folder, which must be
      empty. Uses your Git credential helper or SSH agent; large repositories
      may take a few minutes.
    </p>
    {#if error}
      <p class="gd-error" role="alert">{error.code}: {error.message}</p>
    {/if}
    {#if busy}
      <p class="gd-busy" role="status">Cloning… this may take a few minutes.</p>
    {/if}
    <div class="gd-modal-actions">
      <button type="button" onclick={onCancel} disabled={busy}>Cancel</button>
      <button type="button" onclick={onBrowseFolder} disabled={busy}>Choose folder…</button>
      <button
        type="button"
        onclick={onConfirm}
        disabled={busy || sourceUrl.trim() === "" || folder === ""}
      >
        Clone repository
      </button>
    </div>
  </div>
</div>

<style>
  .gd-modal-backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
    z-index: 20;
  }
  .gd-modal {
    width: 480px;
    max-width: calc(100vw - 48px);
    padding: var(--gd-space-4);
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-panel);
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: var(--gd-font-size-title);
  }
  .gd-modal label {
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-1);
    font-size: var(--gd-font-size-small);
    margin-top: var(--gd-space-2);
  }
  .gd-modal input {
    padding: 6px 10px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-busy {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-3);
  }
  .gd-modal-actions button {
    padding: 6px 14px;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
  }
  .gd-modal-actions button:first-child,
  .gd-modal-actions button:nth-child(2) {
    color: var(--gd-text);
    background: transparent;
    border: 1px solid var(--gd-border);
  }
  .gd-modal-actions button:last-child {
    color: var(--gd-on-accent);
    background: var(--gd-accent);
    border: 0;
  }
  .gd-modal-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  code {
    font-family: var(--gd-font-code);
  }
</style>
