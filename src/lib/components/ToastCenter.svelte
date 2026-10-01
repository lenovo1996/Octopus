<script lang="ts">
  // ToastCenter: transient success/info notices stacked above the status bar.
  // Errors stay inline (bars and dialogs already own them); toasts never do.
  import { dismissToast, toasts } from "../toast";
</script>

<div class="gd-toasts" aria-live="polite" aria-label="Notifications">
  {#each $toasts as toast (toast.id)}
    <div class="gd-toast" class:gd-toast-info={toast.kind === "info"} role="status">
      <span class="gd-toast-mark" aria-hidden="true">{toast.kind === "info" ? "ℹ" : "✓"}</span>
      <span class="gd-toast-message">{toast.message}</span>
      <button type="button" class="gd-toast-dismiss" aria-label="Dismiss notification" onclick={() => dismissToast(toast.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .gd-toasts {
    position: fixed;
    left: var(--gd-space-4);
    bottom: calc(var(--gd-statusbar-height) + var(--gd-space-3));
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: var(--gd-space-2);
    max-width: min(360px, calc(100vw - 32px));
    pointer-events: none;
  }
  .gd-toast {
    display: flex;
    align-items: center;
    gap: var(--gd-space-2);
    padding: var(--gd-space-2) var(--gd-space-3);
    background: var(--gd-surface-raised);
    border: 1px solid var(--gd-border);
    border-left: 3px solid var(--gd-accent);
    border-radius: var(--gd-radius-control);
    box-shadow: 0 8px 24px #0006;
    font-size: var(--gd-font-size);
    color: var(--gd-text);
    pointer-events: auto;
    animation: gd-toast-in 160ms ease-out;
  }
  .gd-toast-info {
    border-left-color: var(--gd-focus);
  }
  .gd-toast-mark {
    flex: none;
    color: var(--gd-accent);
    font-weight: 700;
  }
  .gd-toast-info .gd-toast-mark {
    color: var(--gd-focus);
  }
  .gd-toast-message {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gd-toast-dismiss {
    flex: none;
    background: transparent;
    border: 0;
    border-radius: var(--gd-radius-control);
    color: var(--gd-text-secondary);
    font-size: 16px;
    line-height: 1;
    padding: 0 var(--gd-space-1);
    cursor: pointer;
  }
  .gd-toast-dismiss:hover {
    color: var(--gd-text);
    background: var(--gd-surface-hover);
  }
  .gd-toast-dismiss:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  @keyframes gd-toast-in {
    from { transform: translateY(8px); opacity: 0; }
    to { transform: translateY(0); opacity: 1; }
  }
  @media (prefers-reduced-motion: reduce) {
    .gd-toast { animation: none; }
  }
</style>
