<script lang="ts">
  // GitToolbar: primary Git actions. Branch opens the T10 dialog; Fetch /
  // Pull / Push run T11 background jobs. Stash stays a disabled shell (T12).
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
    onBranches,
    remoteLabel,
    remoteTitle,
    syncDisabled,
    syncDisabledReason,
    jobActive,
    jobLabel,
    jobProgress,
    onFetch,
    onPull,
    onPush,
    onMerge,
    onStash,
    onCancel,
    syncError,
    syncErrorCode,
    syncRetryLabel,
    onRetrySync,
    bitbucketAvailable,
    onConnectBitbucket,
    logOpen,
    onToggleLog,
    logLoading,
    logEntries,
    logError
  }: Props = $props();

  const stashHint =
    "Open the stash dialog (save, apply, pop). Stash needs a trusted repository.";
</script>

<div class="gd-toolbar" role="toolbar" aria-label="Git actions">
  <button type="button" class="gd-tool gd-tool-live" onclick={onBranches} title="Create, switch, and delete branches">
    Branch
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={syncDisabled ? syncDisabledReason : "Fetch from the upstream remote (background job)"}
    onclick={onFetch}
  >
    Fetch
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={syncDisabled ? syncDisabledReason : "Fast-forward pull from the upstream remote"}
    onclick={onPull}
  >
    Pull
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={syncDisabled ? syncDisabledReason : "Push HEAD to the upstream remote (never force)"}
    onclick={onPush}
  >
    Push
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={syncDisabled
      ? syncDisabledReason
      : jobActive
        ? "Wait for the running job first"
        : "Merge a branch with a review stop (never fast-forwards silently)"}
    onclick={onMerge}
  >
    Merge
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled}
    title={syncDisabled ? syncDisabledReason : "Stash working changes (tracked-only by default)"}
    onclick={onStash}
  >
    Stash
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled}
    title={syncDisabled ? syncDisabledReason : stashHint}
    onclick={onStash}
  >
    Pop stash
  </button>
  <span class="gd-remote" title={remoteTitle} role="status">
    {remoteLabel ?? "No upstream"}
  </span>
  {#if bitbucketAvailable}
    <button
      type="button"
      class="gd-tool gd-tool-live gd-bitbucket"
      onclick={onConnectBitbucket}
      disabled={jobActive}
      title="Connect or replace the Bitbucket Cloud API token"
    >
      Bitbucket auth
    </button>
  {/if}
  {#if jobActive}
    <span class="gd-job" role="status">
      {jobLabel}{jobProgress === null ? "…" : ` ${jobProgress}%`}
    </span>
    <button type="button" class="gd-tool gd-tool-live" onclick={onCancel} title="Ask the running job to stop">
      Cancel
    </button>
  {/if}
  <button
    type="button"
    class="gd-tool gd-tool-live gd-log-toggle"
    onclick={onToggleLog}
    aria-expanded={logOpen}
    title="Show the redacted operation log for this repository"
  >
    {logOpen ? "Hide log" : "Sync log"}
  </button>
</div>
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
  .gd-toolbar {
    display: flex;
    align-items: center;
    gap: var(--gd-space-1);
    height: var(--gd-toolbar-height);
    padding: 0 var(--gd-space-3);
    background: var(--gd-panel);
    border-bottom: 1px solid var(--gd-border);
    flex: 0 0 auto;
    overflow: hidden;
  }
  .gd-tool {
    padding: 5px 12px;
    color: var(--gd-text-secondary);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--gd-radius-control);
    font-size: var(--gd-font-size);
    white-space: nowrap;
    cursor: not-allowed;
  }
  .gd-tool:disabled {
    opacity: 0.75;
  }
  .gd-tool-live {
    cursor: pointer;
    color: var(--gd-text);
  }
  .gd-tool-live:disabled {
    cursor: not-allowed;
  }
  .gd-remote {
    margin-left: auto;
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .gd-job {
    font-size: var(--gd-font-size-small);
    color: var(--gd-text);
    white-space: nowrap;
  }
  .gd-log-toggle {
    margin-left: var(--gd-space-1);
  }
  .gd-bitbucket {
    color: var(--gd-accent);
    border-color: color-mix(in srgb, var(--gd-accent) 35%, var(--gd-border));
  }
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
