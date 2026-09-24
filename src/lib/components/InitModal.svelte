<script lang="ts">
  // Init dialog: folder path (from native picker) + branch name + explicit
  // confirmation. Never inits over an existing repository (backend refuses).
  import type { AppError } from "../ipc/types";

  interface Props {
    folder: string;
    branch: string;
    busy: boolean;
    error: AppError | null;
    onBranch: (value: string) => void;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let { folder, branch, busy, error, onBranch, onConfirm, onCancel }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onCancel();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Init repository">
    <h2>Init repository</h2>
    <p class="gd-muted">Folder: <code>{folder}</code></p>
    <label>
      Initial branch
      <input
        type="text"
        value={branch}
        oninput={(e) => onBranch(e.currentTarget.value)}
        placeholder="main"
        aria-label="Initial branch name"
      />
    </label>
    <p class="gd-muted">
      Creates a new Git repository in the chosen folder. Refuses when the folder
      already contains one; existing files are never deleted.
    </p>
    {#if error}
      <p class="gd-error" role="alert">{error.code}: {error.message}</p>
    {/if}
    <div class="gd-modal-actions">
      <button type="button" onclick={onCancel} disabled={busy}>Cancel</button>
      <button type="button" onclick={onConfirm} disabled={busy || branch.trim() === ""}>
        Init repository
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
    width: 440px;
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
  .gd-modal-actions button:first-child {
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
