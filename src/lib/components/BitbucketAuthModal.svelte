<script lang="ts">
  import { onMount } from "svelte";
  import type { AppError } from "../ipc/types";
  import { bitbucketSubmitLabel } from "../sync/bitbucket";

  interface Props {
    remoteUrl: string;
    token: string;
    busy: boolean;
    error: AppError | null;
    retryKind: string | null;
    onToken: (value: string) => void;
    onCreateToken: () => void;
    onSubmit: () => void;
    onClose: () => void;
  }

  let {
    remoteUrl,
    token,
    busy,
    error,
    retryKind,
    onToken,
    onCreateToken,
    onSubmit,
    onClose
  }: Props = $props();
  let tokenInput: HTMLInputElement | undefined;

  onMount(() => tokenInput?.focus());

  function keyDown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !busy) onClose();
  }
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Connect Bitbucket">
    <header>
      <div>
        <span class="gd-provider">BITBUCKET CLOUD</span>
        <h2>Connect Bitbucket</h2>
      </div>
      <button type="button" class="gd-close" onclick={onClose} disabled={busy} aria-label="Close">×</button>
    </header>

    <p class="gd-intro">
      App Passwords stopped working in June 2026. Use a scoped Bitbucket API token for HTTPS Git operations.
    </p>

    <div class="gd-remote" title={remoteUrl}>
      <span>Remote</span>
      <code>{remoteUrl}</code>
    </div>

    <div class="gd-scopes" aria-label="Required token permissions">
      <span>Required permissions</span>
      <strong>Repository: Read</strong>
      <strong>Repository: Write</strong>
    </div>

    <button type="button" class="gd-doc-link" onclick={onCreateToken}>
      Create a scoped API token on Atlassian ↗
    </button>

    <label>
      <span>API token</span>
      <input
        bind:this={tokenInput}
        type="password"
        value={token}
        oninput={(event) => onToken(event.currentTarget.value)}
        autocomplete="new-password"
        spellcheck="false"
        placeholder="Paste token"
        aria-describedby="bitbucket-storage-note"
      />
    </label>

    <p id="bitbucket-storage-note" class="gd-note">
      Octopus sends the token to your configured Git credential helper through stdin. It is never added to the remote URL, app settings, or logs.
    </p>

    {#if error}
      <p class="gd-error" role="alert"><strong>{error.code}</strong> {error.message}</p>
    {/if}

    <footer>
      <button type="button" onclick={onClose} disabled={busy}>Cancel</button>
      <button
        type="button"
        class="gd-primary"
        onclick={onSubmit}
        disabled={busy || token.trim() === ""}
      >
        {busy ? "Saving…" : bitbucketSubmitLabel(retryKind)}
      </button>
    </footer>
  </div>
</div>

<style>
  .gd-modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: rgba(0, 0, 0, 0.58);
  }
  .gd-modal {
    width: min(500px, calc(100vw - 48px));
    padding: var(--gd-space-4);
    color: var(--gd-text);
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-panel);
  }
  header,
  footer,
  .gd-scopes {
    display: flex;
    align-items: center;
  }
  header {
    justify-content: space-between;
    gap: var(--gd-space-3);
  }
  h2 {
    margin: 2px 0 0;
    font-size: var(--gd-font-size-title);
  }
  .gd-provider {
    color: var(--gd-accent);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.09em;
  }
  .gd-close {
    width: 30px;
    height: 30px;
    color: var(--gd-text-secondary);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--gd-radius-control);
    font-size: 20px;
    cursor: pointer;
  }
  .gd-close:hover {
    color: var(--gd-text);
    border-color: var(--gd-border);
  }
  .gd-intro,
  .gd-note {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
    line-height: 1.45;
  }
  .gd-remote {
    display: grid;
    gap: 4px;
    margin: var(--gd-space-3) 0;
    padding: var(--gd-space-2);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    font-size: var(--gd-font-size-small);
  }
  .gd-remote span,
  .gd-scopes > span {
    color: var(--gd-text-secondary);
  }
  .gd-remote code {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gd-scopes {
    flex-wrap: wrap;
    gap: var(--gd-space-1);
    margin-bottom: var(--gd-space-2);
    font-size: var(--gd-font-size-small);
  }
  .gd-scopes > span {
    width: 100%;
  }
  .gd-scopes strong {
    padding: 3px 7px;
    background: color-mix(in srgb, var(--gd-accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--gd-accent) 32%, var(--gd-border));
    border-radius: 999px;
    color: var(--gd-text);
    font-weight: 600;
  }
  .gd-doc-link {
    padding: 0;
    color: var(--gd-accent);
    background: transparent;
    border: 0;
    font-size: var(--gd-font-size-small);
    cursor: pointer;
  }
  label {
    display: grid;
    gap: 6px;
    margin-top: var(--gd-space-3);
    font-size: var(--gd-font-size-small);
    font-weight: 600;
  }
  input {
    padding: 8px 10px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    font: inherit;
    font-weight: 400;
  }
  input:focus {
    border-color: var(--gd-accent);
    outline: 1px solid var(--gd-accent);
  }
  .gd-error {
    padding: var(--gd-space-2);
    color: var(--gd-danger);
    background: color-mix(in srgb, var(--gd-danger) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--gd-danger) 28%, var(--gd-border));
    border-radius: var(--gd-radius-control);
    font-size: var(--gd-font-size-small);
  }
  footer {
    justify-content: flex-end;
    gap: var(--gd-space-2);
    margin-top: var(--gd-space-4);
  }
  footer button {
    padding: 7px 13px;
    color: var(--gd-text);
    background: transparent;
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    cursor: pointer;
  }
  footer .gd-primary {
    color: var(--gd-button-text, #fff);
    background: var(--gd-accent);
    border-color: var(--gd-accent);
    font-weight: 700;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
</style>
