<script lang="ts">
  // Welcome view (no repository open): Open / Clone / Init, recent list,
  // Git preflight line. Clone is T11 — visible but disabled with a reason.
  import type { AppError, PreflightData, RecentEntry } from "../ipc/types";

  interface Props {
    demo: boolean;
    preflight: PreflightData | null;
    preflightError: AppError | null;
    recents: RecentEntry[];
    busy: boolean;
    error: AppError | null;
    onOpen: () => void;
    onInit: () => void;
    onOpenRecent: (path: string) => void;
    onRemoveRecent: (entryId: string) => void;
    onRetryPreflight: () => void;
  }

  let {
    demo,
    preflight,
    preflightError,
    recents,
    busy,
    error,
    onOpen,
    onInit,
    onOpenRecent,
    onRemoveRecent,
    onRetryPreflight
  }: Props = $props();
</script>

<main class="gd-welcome">
  <div class="gd-welcome-card">
    <h1><img src="/brand/octopus-128.png" alt="" width="40" height="40" />Octopus</h1>
    {#if demo}
      <p class="gd-demo-note">Browser preview — static demo data, not native Git.</p>
    {/if}

    <div class="gd-actions">
      <button type="button" onclick={onOpen} disabled={busy}>
        {demo ? "Open demo repository" : "Open repository…"}
      </button>
      <button type="button" onclick={onInit} disabled={busy}>
        {demo ? "Init demo repository" : "Init repository…"}
      </button>
      <button type="button" disabled title="Clone arrives in T11 (remote sync)">
        Clone…
      </button>
    </div>

    {#if error}
      <p class="gd-error" role="alert">
        {error.code}: {error.message}
      </p>
    {/if}

    <section aria-label="Recent repositories">
      <h2>Recent</h2>
      {#if recents.length === 0}
        <p class="gd-muted">No recent repositories yet.</p>
      {:else}
        <ul>
          {#each recents as recent (recent.entryId)}
            <li>
              <button
                type="button"
                class="gd-recent"
                onclick={() => onOpenRecent(recent.displayPath)}
                disabled={busy}
              >
                {recent.displayPath}
              </button>
              <button
                type="button"
                class="gd-remove"
                aria-label={`Remove ${recent.displayPath} from recents`}
                onclick={() => onRemoveRecent(recent.entryId)}
                disabled={busy}>Remove</button
              >
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section aria-label="Git preflight">
      <h2>Environment</h2>
      {#if preflight}
        <p class="gd-muted">
          Git <code>{preflight.gitVersion}</code>
          ({preflight.gitSupported ? "supported" : "unsupported"}) · {preflight.platform} /
          {preflight.arch}
        </p>
      {:else if preflightError}
        <p class="gd-error" role="alert">
          Preflight failed ({preflightError.code}): {preflightError.message}
          <button type="button" onclick={onRetryPreflight}>Retry</button>
        </p>
      {:else}
        <p class="gd-muted" role="status">Checking Git…</p>
      {/if}
    </section>
  </div>
</main>

<style>
  .gd-welcome {
    flex: 1 1 auto;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: var(--gd-space-5);
    overflow-y: auto;
    background: var(--gd-canvas);
  }
  .gd-welcome-card {
    width: 520px;
    max-width: 100%;
    padding: var(--gd-space-5);
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-panel);
  }
  .gd-welcome-card h1 {
    display: flex;
    align-items: center;
    gap: var(--gd-space-2);
    margin: 0 0 var(--gd-space-1);
    font-size: var(--gd-font-size-title);
  }
  .gd-welcome-card h2 {
    margin: var(--gd-space-4) 0 var(--gd-space-2);
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
  }
  .gd-demo-note {
    color: var(--gd-warning);
    font-size: var(--gd-font-size-small);
  }
  .gd-actions {
    display: flex;
    gap: var(--gd-space-2);
    margin: var(--gd-space-3) 0;
  }
  .gd-actions button {
    padding: 7px 14px;
    color: var(--gd-on-accent);
    background: var(--gd-accent);
    border: 0;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
  }
  .gd-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .gd-actions button[title] {
    color: var(--gd-text);
    background: var(--gd-surface-raised);
    border: 1px solid var(--gd-border);
  }
  .gd-welcome ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .gd-welcome li {
    display: flex;
    align-items: center;
    gap: var(--gd-space-2);
    padding: 4px 0;
  }
  .gd-recent {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    color: var(--gd-text);
    background: transparent;
    border: 0;
    cursor: pointer;
    font-size: var(--gd-font-size-small);
    padding: 4px;
  }
  .gd-recent:hover {
    background: var(--gd-surface-hover);
  }
  .gd-remove {
    color: var(--gd-text-secondary);
    background: transparent;
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    font-size: var(--gd-font-size-small);
    padding: 3px 8px;
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  button:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  code {
    font-family: var(--gd-font-code);
  }
</style>
