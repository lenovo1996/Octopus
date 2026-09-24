<script lang="ts">
  // Stash-and-switch dialog: when the worktree is dirty, switching branches
  // (or checking out a commit) offers to stash first instead of failing.
  // The stash is restored manually from Stashes — never auto-popped, so a
  // conflicted pop cannot surprise the user mid-switch. Defaults to Cancel.
  import type { AppError } from "../ipc/types";

  interface Props {
    targetLabel: string;
    dirtySummary: string;
    includeUntracked: boolean;
    busy: boolean;
    error: AppError | null;
    done: string | null;
    onIncludeUntracked: (value: boolean) => void;
    onConfirm: () => void;
    onClose: () => void;
  }

  let {
    targetLabel,
    dirtySummary,
    includeUntracked,
    busy,
    error,
    done,
    onIncludeUntracked,
    onConfirm,
    onClose
  }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape" && !busy) onClose();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Stash and switch">
    <h2>Uncommitted changes</h2>
    {#if done}
      <p class="gd-done" role="status">{done}</p>
    {:else}
      <p class="gd-muted">
        <strong>{dirtySummary}</strong> would be overwritten by switching to
        <code>{targetLabel}</code>. Stash the changes first and restore them
        from Stashes when ready.
      </p>
    {/if}
    {#if error}
      <p class="gd-error" role="alert">{error.code}: {error.message}</p>
    {/if}
    {#if !done}
      <label class="gd-check">
        <input
          type="checkbox"
          checked={includeUntracked}
          onchange={(e) => onIncludeUntracked(e.currentTarget.checked)}
          disabled={busy}
        />
        Include untracked files
      </label>
    {/if}
    <div class="gd-modal-actions">
      {#if done}
        <button type="button" onclick={onClose}>Done</button>
      {:else}
        <button type="button" onclick={onClose} disabled={busy}>Cancel</button>
        <button type="button" class="gd-primary-btn" onclick={onConfirm} disabled={busy}>
          {busy ? "Working…" : "Stash & switch"}
        </button>
      {/if}
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
    max-height: calc(100vh - 96px);
    overflow-y: auto;
    padding: var(--gd-space-4);
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-panel);
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: var(--gd-font-size-title);
  }
  .gd-check {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: var(--gd-space-2);
    font-size: var(--gd-font-size-small);
    margin-top: var(--gd-space-2);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-done {
    color: var(--gd-accent);
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
    color: var(--gd-text);
    background: transparent;
    border: 1px solid var(--gd-border);
  }
  .gd-modal-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .gd-primary-btn {
    border-color: var(--gd-accent) !important;
  }
  code {
    font-family: var(--gd-font-code);
  }
</style>
