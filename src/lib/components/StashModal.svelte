<script lang="ts">
  // Stash dialog (T12): save (tracked-only default, explicit untracked opt-in)
  // plus apply/pop per entry. Entries identify by OID; indices re-resolve.
  import type { AppError, StashEntry } from "../ipc/types";

  interface Props {
    entries: StashEntry[];
    loading: boolean;
    error: AppError | string | null;
    notice: string | null;
    message: string;
    includeUntracked: boolean;
    saveBusy: boolean;
    busyEntry: string | null;
    saveDisabledReason: string | null;
    onMessage: (value: string) => void;
    onIncludeUntracked: (value: boolean) => void;
    onSave: () => void;
    onApply: (entry: StashEntry) => void;
    onPop: (entry: StashEntry) => void;
    onClose: () => void;
  }

  let {
    entries,
    loading,
    error,
    notice,
    message,
    includeUntracked,
    saveBusy,
    busyEntry,
    saveDisabledReason,
    onMessage,
    onIncludeUntracked,
    onSave,
    onApply,
    onPop,
    onClose
  }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  function errorText(e: AppError | string): string {
    return typeof e === "string" ? e : `${e.code}: ${e.message}`;
  }

  function createdLabel(createdAt: number): string {
    if (!createdAt) return "unknown time";
    return new Date(createdAt * 1000).toLocaleString();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal gd-modal-wide" role="dialog" aria-modal="true" aria-label="Stash">
    <h2>Stash</h2>
    {#if error}
      <p class="gd-error" role="alert">{errorText(error)}</p>
    {/if}
    {#if notice}
      <p class="gd-notice" role="status">{notice}</p>
    {/if}

    <section aria-label="Save stash">
      <h3>Save working changes</h3>
      <label class="gd-field">
        <span>Message (optional)</span>
        <input
          type="text"
          value={message}
          maxlength={500}
          placeholder="WIP on current branch"
          oninput={(e) => onMessage(e.currentTarget.value)}
        />
      </label>
      <label class="gd-check">
        <input
          type="checkbox"
          checked={includeUntracked}
          onchange={(e) => onIncludeUntracked(e.currentTarget.checked)}
        />
        Include untracked files (tracked-only by default; ignored files never stashed)
      </label>
      <button
        type="button"
        class="gd-primary"
        disabled={saveBusy || saveDisabledReason !== null}
        title={saveDisabledReason ?? "Stash current changes (apply without --index on restore)"}
        onclick={onSave}
      >
        {saveBusy ? "Stashing…" : "Stash"}
      </button>
    </section>

    <section aria-label="Stashed entries">
      <h3>Stashed ({entries.length})</h3>
      {#if loading && entries.length === 0}
        <p class="gd-muted" role="status">Loading stash…</p>
      {:else if entries.length === 0}
        <p class="gd-muted">No stashed changes.</p>
      {:else}
        <ul class="gd-stash-list">
          {#each entries as entry (entry.stashId + entry.oid)}
            <li>
              <div class="gd-stash-meta">
                <span class="gd-stash-id">{entry.stashId}</span>
                <span class="gd-stash-label">{entry.label}</span>
                <span class="gd-muted">{entry.oid.slice(0, 8)} · {createdLabel(entry.createdAt)}</span>
              </div>
              <div class="gd-stash-actions">
                <button
                  type="button"
                  disabled={busyEntry !== null}
                  title="Restore these changes, keep the entry"
                  onclick={() => onApply(entry)}
                >
                  {busyEntry === `apply:${entry.stashId}` ? "Applying…" : "Apply"}
                </button>
                <button
                  type="button"
                  disabled={busyEntry !== null}
                  title="Restore these changes, drop the entry on success"
                  onclick={() => onPop(entry)}
                >
                  {busyEntry === `pop:${entry.stashId}` ? "Popping…" : "Pop"}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <div class="gd-modal-foot">
      <button type="button" onclick={onClose}>Close</button>
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
    max-width: 640px;
    max-height: 80vh;
    overflow-y: auto;
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-3);
    font-size: 15px;
  }
  .gd-modal h3 {
    margin: var(--gd-space-3) 0 var(--gd-space-2);
    font-size: 13px;
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-notice {
    color: var(--gd-warning);
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
    margin-bottom: var(--gd-space-2);
    font-size: var(--gd-font-size-small);
  }
  .gd-field input {
    background: var(--gd-background);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    color: var(--gd-text);
    padding: 6px 8px;
  }
  .gd-check {
    display: flex;
    gap: var(--gd-space-2);
    align-items: flex-start;
    font-size: var(--gd-font-size-small);
    margin-bottom: var(--gd-space-2);
  }
  .gd-primary {
    padding: 6px 14px;
    cursor: pointer;
  }
  .gd-stash-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-2);
  }
  .gd-stash-list li {
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2) var(--gd-space-3);
  }
  .gd-stash-meta {
    display: flex;
    gap: var(--gd-space-2);
    align-items: baseline;
    font-size: var(--gd-font-size-small);
  }
  .gd-stash-id {
    color: var(--gd-text-secondary);
    white-space: nowrap;
  }
  .gd-stash-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gd-stash-actions {
    display: flex;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-1);
  }
  .gd-modal-foot {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--gd-space-3);
  }
</style>
