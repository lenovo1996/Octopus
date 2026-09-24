<script lang="ts">
  // Branch workflow dialog (T10): create (with optional switch), switch
  // local refs, explicit-name tracking for remotes, and two-step safe
  // delete behind a confirmation token. Delete defaults to Cancel.
  import type { AppError, RefItem } from "../ipc/types";

  interface DeleteConfirm {
    refId: string;
    summary: string;
  }

  interface Props {
    localRefs: RefItem[];
    remoteRefs: RefItem[];
    startOidShort: string | null;
    startSource: "HEAD" | "commit";
    newName: string;
    switchAfter: boolean;
    trackName: string;
    busy: boolean;
    error: AppError | string | null;
    deleteConfirm: DeleteConfirm | null;
    onName: (value: string) => void;
    onSwitchAfter: (value: boolean) => void;
    onTrackName: (value: string) => void;
    onCreate: () => void;
    onSwitch: (refId: string) => void;
    onAskDelete: (refId: string) => void;
    onConfirmDelete: () => void;
    onCancelDelete: () => void;
    onTrack: (refId: string) => void;
    onClose: () => void;
  }

  let {
    localRefs,
    remoteRefs,
    startOidShort,
    startSource,
    newName,
    switchAfter,
    trackName,
    busy,
    error,
    deleteConfirm,
    onName,
    onSwitchAfter,
    onTrackName,
    onCreate,
    onSwitch,
    onAskDelete,
    onConfirmDelete,
    onCancelDelete,
    onTrack,
    onClose
  }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      if (deleteConfirm) onCancelDelete();
      else onClose();
    }
  }

  function errorText(e: AppError | string): string {
    return typeof e === "string" ? e : `${e.code}: ${e.message}`;
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal gd-modal-wide" role="dialog" aria-modal="true" aria-label="Branches">
    <h2>Branches</h2>
    {#if error}
      <p class="gd-error" role="alert">{errorText(error)}</p>
    {/if}

    {#if deleteConfirm}
      <section aria-label="Confirm delete">
        <h3>Delete this branch?</h3>
        <pre class="gd-summary">{deleteConfirm.summary}</pre>
        <div class="gd-modal-actions">
          <button type="button" onclick={onCancelDelete} disabled={busy}>Cancel</button>
          <button type="button" class="gd-danger-btn" onclick={onConfirmDelete} disabled={busy}>
            {busy ? "Deleting…" : "Delete branch"}
          </button>
        </div>
      </section>
    {:else}
      <section aria-label="Create branch">
        <h3>New branch</h3>
        {#if startOidShort}
          <p class="gd-muted">Starting from {startSource} <code>{startOidShort}</code>.</p>
          <label>
            Branch name
            <input
              type="text"
              value={newName}
              oninput={(e) => onName(e.currentTarget.value)}
              placeholder="feature/name"
              aria-label="New branch name"
            />
          </label>
          <label class="gd-check">
            <input
              type="checkbox"
              checked={switchAfter}
              onchange={(e) => onSwitchAfter(e.currentTarget.checked)}
            />
            Switch after create
          </label>
          <div class="gd-modal-actions">
            <button type="button" onclick={onCreate} disabled={busy || newName.trim() === ""}>
              {busy ? "Creating…" : "Create branch"}
            </button>
          </div>
        {:else}
          <p class="gd-muted">No commits yet — create the first commit before branching.</p>
        {/if}
      </section>

      <section aria-label="Local branches">
        <h3>Local ({localRefs.length})</h3>
        {#if localRefs.length === 0}
          <p class="gd-muted">No local branches.</p>
        {:else}
          <ul>
            {#each localRefs as ref (ref.refId)}
              <li>
                <code>{ref.label}</code>
                {#if ref.current}
                  <span class="gd-muted">current</span>
                {:else}
                  <button type="button" class="gd-link" onclick={() => onSwitch(ref.refId)} disabled={busy}>
                    Switch
                  </button>
                {/if}
                {#if ref.checkedOutElsewhere}
                  <span class="gd-muted">checked out elsewhere</span>
                {:else if !ref.current}
                  <button type="button" class="gd-link gd-danger" onclick={() => onAskDelete(ref.refId)} disabled={busy}>
                    Delete
                  </button>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      {#if remoteRefs.length > 0}
        <section aria-label="Remote branches">
          <h3>Remote tracking ({remoteRefs.length})</h3>
          <ul>
            {#each remoteRefs as ref (ref.refId)}
              <li>
                <code>{ref.label}</code>
                <input
                  type="text"
                  value={trackName}
                  oninput={(e) => onTrackName(e.currentTarget.value)}
                  aria-label={`Local name for ${ref.label}`}
                />
                <button type="button" class="gd-link" onclick={() => onTrack(ref.refId)} disabled={busy || trackName.trim() === ""}>
                  Track
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <div class="gd-modal-actions">
        <button type="button" onclick={onClose} disabled={busy}>Close</button>
      </div>
    {/if}
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
  .gd-modal-wide {
    width: 560px;
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: var(--gd-font-size-title);
  }
  .gd-modal h3 {
    margin: var(--gd-space-3) 0 var(--gd-space-1);
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
  }
  .gd-modal label {
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-1);
    font-size: var(--gd-font-size-small);
    margin-top: var(--gd-space-2);
  }
  .gd-check {
    flex-direction: row !important;
    align-items: center;
  }
  .gd-modal input[type="text"] {
    padding: 6px 10px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
  }
  .gd-modal ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .gd-modal li {
    display: flex;
    align-items: center;
    gap: var(--gd-space-2);
    padding: 4px 0;
    font-size: var(--gd-font-size-small);
  }
  .gd-link {
    color: var(--gd-accent);
    background: transparent;
    border: 0;
    cursor: pointer;
    padding: 2px 4px;
  }
  .gd-link:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
  .gd-danger {
    color: var(--gd-danger);
  }
  .gd-summary {
    font-size: var(--gd-font-size-small);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2);
    white-space: pre-wrap;
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
    color: var(--gd-text);
    background: transparent;
    border: 1px solid var(--gd-border);
  }
  .gd-modal-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .gd-danger-btn {
    color: white !important;
    background: var(--gd-danger) !important;
    border: 0 !important;
  }
  code {
    font-family: var(--gd-font-code);
  }
</style>
