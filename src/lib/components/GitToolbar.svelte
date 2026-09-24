<script lang="ts">
  // GitToolbar: sync error/log panels only. Action buttons live in GitActions
  // (centered in the TopBar) with per-button job spinners. Props stay wide so
  // existing fixtures keep compiling.
  import type { OperationLogEntry } from "../ipc/types";

  interface Props {
    onBranches: () => void;
    remoteLabel: string | null;
    remoteTitle: string;
    syncDisabled: boolean;
    syncDisabledReason: string;
    jobActive: boolean;
    jobLabel: string;
    jobProgress: number | null;
    onFetch: () => void;
    onPull: () => void;
    onPush: () => void;
    onMerge: () => void;
    onStash: () => void;
    onCancel: () => void;
    syncError: string | null;
    syncErrorCode: string | null;
    syncRetryLabel: string | null;
    onRetrySync: () => void;
    bitbucketAvailable: boolean;
    onConnectBitbucket: () => void;
    logOpen: boolean;
    onToggleLog: () => void;
    logLoading: boolean;
    logEntries: OperationLogEntry[];
    logError: string | null;
  }

  let {
    jobActive,
    syncError,
    syncErrorCode,
    syncRetryLabel,
    onRetrySync,
    bitbucketAvailable,
    onConnectBitbucket,
    logOpen,
    logLoading,
    logEntries,
    logError
  }: Props = $props();
</script>

{#if syncError}
  <div class="gd-sync-error" role="alert">
    <div>
      {#if syncErrorCode}<strong>{syncErrorCode}</strong>{/if}
      <span>{syncError}</span>
    </div>
    <div class="gd-sync-actions">
      {#if bitbucketAvailable && syncErrorCode === "AUTH_REQUIRED"}
        <button type="button" class="gd-connect" onclick={onConnectBitbucket} disabled={jobActive}>Connect Bitbucket</button>
      {/if}
      {#if syncRetryLabel}
        <button type="button" onclick={onRetrySync} disabled={jobActive}>{syncRetryLabel}</button>
      {/if}
    </div>
  </div>
{/if}
{#if logOpen}
  <div class="gd-sync-log" aria-label="Operation log">
    {#if logLoading && logEntries.length === 0}
      <div class="gd-log-row gd-muted">Loading operation log…</div>
    {:else if logError}
      <div class="gd-log-row" role="alert">{logError}</div>
    {:else if logEntries.length === 0}
      <div class="gd-log-row gd-muted">No remote operations recorded yet.</div>
    {:else}
      {#each logEntries as entry (entry.seq)}
        <div class="gd-log-row">
          <span class="gd-log-kind">{entry.kind}</span>
          <span class="gd-log-summary">{entry.summary}</span>
          <span class="gd-log-outcome gd-outcome-{entry.outcome}">{entry.outcome}</span>
        </div>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .gd-sync-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gd-space-3);
    padding: 4px var(--gd-space-3);
    font-size: var(--gd-font-size-small);
    color: var(--gd-danger, #f47067);
    background: var(--gd-panel);
    border-bottom: 1px solid var(--gd-border);
  }
  .gd-sync-error > div { display: flex; align-items: baseline; gap: var(--gd-space-2); min-width: 0; }
  .gd-sync-error .gd-sync-actions { flex: 0 0 auto; align-items: center; }
  .gd-sync-error strong { flex: 0 0 auto; font: 10px var(--gd-font-code); }
  .gd-sync-error span { color: var(--gd-text-secondary); }
  .gd-sync-error button {
    flex: 0 0 auto;
    padding: 3px 9px;
    border: 1px solid color-mix(in srgb, var(--gd-danger) 45%, var(--gd-border));
    border-radius: var(--gd-radius-control);
    color: var(--gd-text);
    background: transparent;
    cursor: pointer;
  }
  .gd-sync-error button:hover:not(:disabled) { background: var(--gd-surface-hover); }
  .gd-sync-error .gd-connect {
    color: var(--gd-text);
    background: color-mix(in srgb, var(--gd-accent) 16%, transparent);
    border-color: color-mix(in srgb, var(--gd-accent) 48%, var(--gd-border));
  }
  .gd-sync-error button:disabled { opacity: .55; cursor: not-allowed; }
  .gd-sync-log {
    max-height: 160px;
    overflow-y: auto;
    background: var(--gd-panel);
    border-bottom: 1px solid var(--gd-border);
    font-size: var(--gd-font-size-small);
  }
  .gd-log-row {
    display: flex;
    gap: var(--gd-space-2);
    padding: 3px var(--gd-space-3);
    align-items: baseline;
  }
  .gd-log-kind {
    min-width: 48px;
    color: var(--gd-text-secondary);
  }
  .gd-log-summary {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gd-log-outcome {
    text-transform: uppercase;
  }
  .gd-outcome-ok {
    color: var(--gd-success, #7ee787);
  }
  .gd-outcome-error {
    color: var(--gd-danger, #f47067);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
  }
</style>
