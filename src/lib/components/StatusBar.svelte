<script lang="ts">
  // StatusBar: operation state, selection summary, preflight version.
  interface Props {
    operation: string;
    selectionSummary: string;
    gitVersion: string | null;
  }

  let { operation, selectionSummary, gitVersion }: Props = $props();
</script>

<footer class="gd-statusbar" aria-label="Status">
  <span class="gd-op" role="status">{operation}</span>
  <span class="gd-selection">{selectionSummary}</span>
  <span class="gd-git">{gitVersion ? `Git ${gitVersion}` : "Git: unknown"}</span>
</footer>

<style>
  .gd-statusbar {
    display: flex;
    align-items: center;
    gap: var(--gd-space-4);
    height: var(--gd-statusbar-height);
    padding: 0 var(--gd-space-3);
    background: var(--gd-panel);
    border-top: 1px solid var(--gd-border);
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
    flex: 0 0 auto;
    overflow: hidden;
    white-space: nowrap;
  }
  .gd-selection {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .gd-git {
    margin-left: auto;
  }
</style>
