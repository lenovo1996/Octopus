<script lang="ts">
  // Help dialog (T14): keyboard shortcuts with input scope, diagnostics
  // from preflight, and the redaction promise.
  interface Props {
    gitVersion: string | null;
    platform: string | null;
    mode: string;
    onClose: () => void;
  }

  let { gitVersion, platform, mode, onClose }: Props = $props();

  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  const shortcuts: [string, string][] = [
    ["Ctrl/⌘ F", "Focus history search"],
    ["Ctrl/⌘ O", "Open repository"],
    ["Ctrl/⌘ Tab · Shift to reverse", "Switch repository tabs"],
    ["Ctrl/⌘ W", "Close repository tab; preserve its draft"],
    ["Ctrl/⌘ R", "Refresh snapshot and listings"],
    ["Esc", "Close dialog, or back to Working changes"]
  ];
</script>

<svelte:window onkeydown={keyDown} />

<div class="gd-modal-backdrop">
  <div class="gd-modal" role="dialog" aria-modal="true" aria-label="Help and shortcuts">
    <h2>Help &amp; shortcuts</h2>
    <p class="gd-muted">Shortcuts never fire while typing in a field — except Esc, which leaves the field first.</p>
    <ul class="gd-keys">
      {#each shortcuts as [keys, action]}
        <li><code>{keys}</code> — {action}</li>
      {/each}
    </ul>
    <h3>Diagnostics</h3>
    <dl class="gd-meta-list">
      <dt>Mode</dt>
      <dd>{mode}</dd>
      <dt>Git</dt>
      <dd>{gitVersion ?? "unknown"}</dd>
      <dt>Platform</dt>
      <dd>{platform ?? "unknown"}</dd>
    </dl>
    <h3>Safety notes</h3>
    <ul class="gd-notes">
      <li>Remote URLs in logs are redacted (no passwords or tokens).</li>
      <li>Push never forces; merges never fast-forward silently.</li>
      <li>Abort is offered only for merges the app started.</li>
    </ul>
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
    min-width: 380px;
    max-width: 520px;
    max-height: 80vh;
    overflow-y: auto;
  }
  .gd-modal h2 {
    margin: 0 0 var(--gd-space-2);
    font-size: 15px;
  }
  .gd-modal h3 {
    margin: var(--gd-space-3) 0 var(--gd-space-1);
    font-size: 13px;
  }
  .gd-muted {
    color: var(--gd-text-secondary);
    font-size: var(--gd-font-size-small);
  }
  .gd-keys,
  .gd-notes {
    margin: var(--gd-space-1) 0;
    padding-left: var(--gd-space-4);
    font-size: var(--gd-font-size-small);
  }
  .gd-keys code {
    font-family: var(--gd-font-code);
  }
  .gd-meta-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px var(--gd-space-3);
    font-size: var(--gd-font-size-small);
    margin: 0;
  }
  .gd-meta-list dt {
    color: var(--gd-text-secondary);
  }
  .gd-meta-list dd {
    margin: 0;
  }
  .gd-modal-foot {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--gd-space-3);
  }
</style>
