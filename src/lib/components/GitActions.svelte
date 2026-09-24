<script lang="ts">
  // Primary Git action buttons. Rendered centered in the TopBar; the slim
  // GitToolbar below keeps remote status, job state and log panels.
  interface Props {
    onBranches: () => void;
    syncDisabled: boolean;
    syncDisabledReason: string;
    jobActive: boolean;
    /** Kind of the running sync job ("fetch" | "pull" | "push"); its button spins. */
    activeJobKind: string | null;
    onFetch: () => void;
    onPull: () => void;
    onPush: () => void;
    onMerge: () => void;
    onStash: () => void;
    onCancel: () => void;
    bitbucketAvailable: boolean;
    onConnectBitbucket: () => void;
    logOpen: boolean;
    onToggleLog: () => void;
  }

  let {
    onBranches,
    syncDisabled,
    syncDisabledReason,
    jobActive,
    activeJobKind,
    onFetch,
    onPull,
    onPush,
    onMerge,
    onStash,
    onCancel,
    bitbucketAvailable,
    onConnectBitbucket,
    logOpen,
    onToggleLog
  }: Props = $props();

  function spinning(kind: "fetch" | "pull" | "push"): boolean {
    return jobActive && activeJobKind === kind;
  }
</script>

<div class="gd-tool-actions" role="toolbar" aria-label="Git actions">
  <button type="button" class="gd-tool gd-tool-live" onclick={onBranches} title="Create, switch, and delete branches">
    Branch
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={spinning("fetch") ? "Fetching from the upstream remote…" : syncDisabled ? syncDisabledReason : "Fetch from the upstream remote (background job)"}
    aria-label={spinning("fetch") ? "Fetching" : "Fetch"}
    onclick={onFetch}
  >
    {#if spinning("fetch")}<span class="gd-spin" aria-hidden="true">⟳</span>{:else}Fetch{/if}
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={spinning("pull") ? "Pulling from the upstream remote…" : syncDisabled ? syncDisabledReason : "Fast-forward pull from the upstream remote"}
    aria-label={spinning("pull") ? "Pulling" : "Pull"}
    onclick={onPull}
  >
    {#if spinning("pull")}<span class="gd-spin" aria-hidden="true">⟳</span>{:else}Pull{/if}
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    disabled={syncDisabled || jobActive}
    title={spinning("push") ? "Pushing to the upstream remote…" : syncDisabled ? syncDisabledReason : "Push HEAD to the upstream remote (never force)"}
    aria-label={spinning("push") ? "Pushing" : "Push"}
    onclick={onPush}
  >
    {#if spinning("push")}<span class="gd-spin" aria-hidden="true">⟳</span>{:else}Push{/if}
  </button>
  {#if jobActive}
    <button type="button" class="gd-tool gd-tool-live" onclick={onCancel} title="Ask the running job to stop">
      Cancel
    </button>
  {/if}
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
    title={syncDisabled ? syncDisabledReason : "Open the stash dialog (save, apply, pop). Stash needs a trusted repository."}
    onclick={onStash}
  >
    Pop stash
  </button>
  <button
    type="button"
    class="gd-tool gd-tool-live"
    onclick={onToggleLog}
    aria-expanded={logOpen}
    title="Show the redacted operation log for this repository"
  >
    {logOpen ? "Hide log" : "Sync log"}
  </button>
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
</div>

<style>
  .gd-tool-actions {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--gd-space-1);
    min-width: 0;
    overflow-x: auto;
  }
  .gd-tool {
    padding: 4px 10px;
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
  .gd-tool-live:hover {
    background: var(--gd-surface-hover);
  }
  .gd-tool-live:disabled {
    cursor: not-allowed;
  }
  .gd-tool:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-bitbucket {
    color: var(--gd-accent);
    border-color: color-mix(in srgb, var(--gd-accent) 35%, var(--gd-border));
  }
  .gd-spin {
    display: inline-block;
    animation: gd-rotate 1s linear infinite;
  }
  @keyframes gd-rotate {
    to { transform: rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .gd-spin { animation: none; }
  }
</style>
