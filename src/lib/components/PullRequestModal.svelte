<script lang="ts">
  // Pull request dialog: title, description, fixed source branch, editable
  // target branch. Submit creates via the hosting provider (Bitbucket /
  // GitHub / GitLab); success shows the PR link.
  import type { AppError, PullRequestResult } from "../ipc/types";

  interface Props {
    sourceLabel: string;
    targetSuggestions: string[];
    targetBranch: string;
    title: string;
    description: string;
    remoteLabel: string | null;
    busy: boolean;
    error: AppError | string | null;
    canSubmit: boolean;
    submitHint: string;
    result: PullRequestResult | null;
    onTarget: (value: string) => void;
    onTitle: (value: string) => void;
    onDescription: (value: string) => void;
    onSubmit: () => void;
    onOpenUrl: (url: string) => void;
    onClose: () => void;
  }

  let {
    sourceLabel,
    targetSuggestions,
    targetBranch,
    title,
    description,
    remoteLabel,
    busy,
    error,
    canSubmit,
    submitHint,
    result,
    onTarget,
    onTitle,
    onDescription,
    onSubmit,
    onOpenUrl,
    onClose
  }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  function errorText(e: AppError | string): string {
    return typeof e === "string" ? e : `${e.code}: ${e.message}`;
  }

  function providerName(provider: string): string {
    return provider === "gitlab" ? "merge request" : "pull request";
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Create pull request">
    <h2>Create pull request</h2>
    {#if error}
      <p class="gd-error" role="alert">{errorText(error)}</p>
    {/if}
    {#if result}
      <p class="gd-success" role="status">
        {providerName(result.provider)} {result.reference} created on {result.provider}.
      </p>
      <p class="gd-muted gd-url">{result.url}</p>
      <div class="gd-modal-foot">
        <button type="button" onclick={onClose}>Close</button>
        <button type="button" class="gd-primary" onclick={() => onOpenUrl(result.url)}>
          Open in browser
        </button>
      </div>
    {:else}
      <p class="gd-muted">
        From <strong>{sourceLabel}</strong>{#if remoteLabel} via {remoteLabel}{/if}.
        Uses your saved Git credential for the remote host; nothing is stored by Octopus.
      </p>
      <label class="gd-field">
        <span>Title</span>
        <input
          value={title}
          oninput={(e) => onTitle(e.currentTarget.value)}
          placeholder="Summarize the change"
          maxlength={512}
          aria-label="Pull request title"
        />
      </label>
      <label class="gd-field">
        <span>Description</span>
        <textarea
          value={description}
          oninput={(e) => onDescription(e.currentTarget.value)}
          placeholder="What does this change do? (optional)"
          rows={4}
          aria-label="Pull request description"
        ></textarea>
      </label>
      <label class="gd-field">
        <span>Target branch</span>
        <input
          value={targetBranch}
          oninput={(e) => onTarget(e.currentTarget.value)}
          list="gd-pr-targets"
          aria-label="Target branch"
        />
        <datalist id="gd-pr-targets">
          {#each targetSuggestions as suggestion (suggestion)}
            <option value={suggestion}></option>
          {/each}
        </datalist>
      </label>
      <div class="gd-modal-foot">
        <button type="button" onclick={onClose}>Cancel</button>
        <button type="button" class="gd-primary" disabled={!canSubmit || busy} title={submitHint} onclick={onSubmit}>
          {busy ? "Creating…" : "Create pull request"}
        </button>
      </div>
    {/if}
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
    width: min(560px, 90vw);
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-3);
    font-size: 15px;
  }
  .gd-error {
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
  }
  .gd-success {
    color: var(--gd-accent);
    font-size: var(--gd-font-size-small);
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-url {
    overflow-wrap: anywhere;
  }
  .gd-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: var(--gd-space-2) 0;
    font-size: var(--gd-font-size-small);
  }
  .gd-field input,
  .gd-field textarea {
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    color: var(--gd-text);
    padding: 7px 8px;
    font: var(--gd-font-size-small)/1.4 var(--gd-font-ui);
    resize: vertical;
  }
  .gd-modal-foot {
    display: flex;
    justify-content: flex-end;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-3);
  }
  .gd-modal-foot button {
    background: var(--gd-panel);
    color: var(--gd-text);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: 6px 12px;
    cursor: pointer;
    font: inherit;
  }
  .gd-modal-foot .gd-primary {
    background: var(--gd-accent);
    border-color: var(--gd-accent);
    color: var(--gd-on-accent);
  }
  .gd-modal-foot button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
