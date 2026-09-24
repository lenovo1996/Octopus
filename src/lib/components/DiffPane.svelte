<script lang="ts">
  import type { DiffState } from "../diff/controller";
  import { highlightDiffLine } from "../diff/highlight";

  let { state, onClose, onRetry, canMutateHunks = false, hunkMutationHint = null, mutationBusy = false,
    mutationError = null, onStageHunk = () => {}, onDiscardHunk = () => {} }: {
    state: DiffState;
    onClose: () => void;
    onRetry: () => void;
    canMutateHunks?: boolean;
    hunkMutationHint?: string | null;
    mutationBusy?: boolean;
    mutationError?: string | null;
    onStageHunk?: (hunkId: string) => void;
    onDiscardHunk?: (hunkId: string) => void;
  } = $props();

  const showHunkActions = $derived(state.selection?.target.kind === "worktree");

  const source = $derived.by(() => {
    const target = state.selection?.target;
    if (target?.kind === "commit") {
      return `Commit ${target.oid.slice(0, 8)} · ${target.parentIndex === null ? "root" : `parent ${target.parentIndex + 1}`}`;
    }
    return target?.kind === "index" ? "Staged · HEAD → index" : "Unstaged · index → working tree";
  });
</script>

<section class="gd-diff-pane" aria-label="File diff" aria-busy={state.loading}>
  <header>
    <div class="gd-diff-title">
      <span class="gd-source">{source}</span>
      <h2 title={state.selection?.path}>{state.selection?.path}</h2>
    </div>
    <button type="button" class="gd-close" onclick={onClose} aria-label="Close diff" title="Close diff (Escape)">×</button>
  </header>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex: scrollable diff must be reachable for keyboard scrolling -->
  <div class="gd-diff-body" tabindex="0" role="region" aria-label="Diff content">
    {#if state.loading}
      <p class="gd-message" role="status">Loading diff…</p>
    {:else if state.error}
      <div class="gd-message" role="alert">
        <p>Diff failed ({state.error.code}): {state.error.message}</p>
        <button type="button" class="gd-retry" onclick={onRetry}>Retry</button>
      </div>
    {:else if state.doc}
      {@const doc = state.doc}
      {#if mutationError}<p class="gd-action-error" role="alert">{mutationError}</p>{/if}
      {#if doc.kind === "text"}
        <p class="gd-summary"><span class="gd-added">+{doc.additions ?? 0}</span> <span class="gd-deleted">−{doc.deletions ?? 0}</span> · {doc.hunks.length} {doc.hunks.length === 1 ? "hunk" : "hunks"} · Unified diff</p>
        {#if showHunkActions && hunkMutationHint}<p class="gd-partial-note">{hunkMutationHint}</p>{/if}
        {#if doc.hunks.length === 0}
          <p class="gd-message">No content changes.</p>
        {/if}
        <div class="gd-diff-code">
          {#each doc.hunks as hunk, index (index)}
            <div class="gd-hunk">
              <code>{hunk.header}</code>
              {#if showHunkActions}
                <div class="gd-hunk-actions" aria-label={`Actions for hunk ${index + 1}`}>
                  <button type="button" class="gd-stage-hunk" disabled={!canMutateHunks || mutationBusy} title={hunkMutationHint ?? "Stage this hunk"} onclick={() => onStageHunk(hunk.hunkId)}>
                    <span aria-hidden="true">+</span> Stage hunk
                  </button>
                  <button type="button" class="gd-discard-hunk" disabled={!canMutateHunks || mutationBusy} title={hunkMutationHint ?? "Discard this hunk"} onclick={() => onDiscardHunk(hunk.hunkId)}>
                    <span aria-hidden="true">↶</span> Discard hunk
                  </button>
                </div>
              {/if}
            </div>
            {#each hunk.lines as line, i (i)}
              <div class="gd-line" class:gd-add={line.kind === "add"} class:gd-delete={line.kind === "delete"}>
                <span class="gd-lineno">{line.oldLine ?? ""}</span>
                <span class="gd-lineno">{line.newLine ?? ""}</span>
                <span class="gd-sign" aria-hidden="true">{line.kind === "add" ? "+" : line.kind === "delete" ? "−" : line.kind === "noNewline" ? "\\" : " "}</span>
                {#if line.kind === "noNewline"}
                  <code>No newline at end of file</code>
                {:else}
                  <!-- highlightDiffLine escapes first, so {@html} cannot inject markup -->
                  <code>{@html highlightDiffLine(line.text, state.selection?.path ?? "")}</code>
                {/if}
              </div>
            {/each}
          {/each}
        </div>
      {:else}
        <p class="gd-message">{doc.reason ?? `${doc.kind} file: no text preview.`}</p>
      {/if}
      {#if doc.truncated}
        <p class="gd-message">Preview truncated — open the file externally for the rest.</p>
      {/if}
    {/if}
  </div>
</section>

<style>
  .gd-diff-pane { display: flex; flex-direction: column; height: 100%; min-width: 0; min-height: 0; background: var(--gd-canvas); }
  header { display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-bottom: 1px solid var(--gd-border); background: var(--gd-panel); }
  .gd-diff-title { flex: 1; min-width: 0; }
  .gd-source { color: var(--gd-text-secondary); font-size: var(--gd-font-size-small); }
  h2 { margin: 5px 0 0; font: 500 var(--gd-font-size) var(--gd-font-code); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { color: var(--gd-text); background: transparent; border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); cursor: pointer; }
  button:hover { background: var(--gd-surface-hover); }
  button:focus-visible, .gd-diff-body:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-close { flex: 0 0 32px; height: 32px; font-size: 24px; line-height: 1; }
  .gd-retry { padding: 5px 12px; }
  .gd-diff-body { flex: 1; min-height: 0; overflow: auto; }
  .gd-message, .gd-summary { margin: 0; padding: 16px; color: var(--gd-text-secondary); }
  .gd-summary { font-variant-numeric: tabular-nums; font-size: var(--gd-font-size-small); }
  .gd-action-error { position: sticky; top: 0; z-index: 3; margin: 0; padding: 9px 16px; color: var(--gd-danger); background: color-mix(in srgb, var(--gd-danger) 10%, var(--gd-canvas)); border-bottom: 1px solid color-mix(in srgb, var(--gd-danger) 30%, var(--gd-border)); font-size: 11px; }
  .gd-partial-note { margin: -8px 16px 12px; padding: 8px 10px; color: var(--gd-text-secondary); background: var(--gd-surface-raised); border-left: 2px solid var(--gd-warning); font-size: 11px; }
  .gd-added { color: var(--gd-accent); }
  .gd-deleted { color: var(--gd-danger); }
  .gd-diff-code { min-width: 100%; width: max-content; font: var(--gd-font-size-small)/24px var(--gd-font-code); tab-size: 4; }
  .gd-hunk { position: sticky; left: 0; display: flex; align-items: center; justify-content: space-between; gap: 24px; min-width: 100%; padding: 4px 8px 4px 16px; color: var(--gd-text-secondary); background: var(--gd-surface-raised); border-block: 1px solid color-mix(in srgb, var(--gd-border) 72%, transparent); }
  .gd-hunk>code { white-space: pre; }
  .gd-hunk-actions { display: flex; gap: 5px; margin-left: auto; font-family: var(--gd-font-ui); }
  .gd-hunk-actions button { display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 8px; background: var(--gd-panel); font-size: 10px; white-space: nowrap; }
  .gd-hunk-actions button:disabled { opacity: .42; cursor: not-allowed; }
  .gd-stage-hunk:not(:disabled):hover { color: var(--gd-accent); border-color: var(--gd-accent); }
  .gd-discard-hunk:not(:disabled):hover { color: var(--gd-danger); border-color: var(--gd-danger); }
  .gd-line { display: flex; min-height: 24px; padding-right: 16px; }
  .gd-add { background: var(--gd-diff-add-bg); }
  .gd-delete { background: var(--gd-diff-delete-bg); }
  .gd-lineno { flex: 0 0 5ch; text-align: right; padding-right: 1ch; color: var(--gd-text-secondary); user-select: none; font-variant-numeric: tabular-nums; }
  .gd-sign { flex: 0 0 3ch; text-align: center; user-select: none; }
  code { font: inherit; white-space: pre; }
  /* Token spans come from {@html}, so selectors must be global to apply. */
  .gd-diff-code :global(.tok-kw) { color: var(--gd-focus); }
  .gd-diff-code :global(.tok-str) { color: var(--gd-warning); }
  .gd-diff-code :global(.tok-num) { color: var(--gd-lane-4); }
  .gd-diff-code :global(.tok-com) { color: var(--gd-text-secondary); font-style: italic; }
</style>
