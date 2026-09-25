<script lang="ts">
  const summaryId = $props.id();
  import type { AppError, ChangedFile, DiffTarget } from "../ipc/types";
  import { partitionStatus } from "../status/partition";
  import { neighborRowIndex } from "./file-row-nav";
  import FileChangeRow from "./FileChangeRow.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import { pointFromContextEvent, type ContextMenuItem } from "../context-menu/model";
  type FileSide = "worktree" | "index";

  let { branchName, files, loading, error, trustBlocked, selectedTarget, indexBusy, indexError,
    subject, body, commitBusy, commitError, identityLabel, headDetached, onRefresh, onOpen,
    onStage, onUnstage, onDiscard, onDiscardAll, onSubject, onBody, onCommit, onConflicts }: {
    branchName: string; files: ChangedFile[] | null; loading: boolean; error: AppError | null; trustBlocked: boolean;
    selectedTarget: DiffTarget | null; indexBusy: boolean; indexError: AppError | null;
    subject: string; body: string; commitBusy: boolean; commitError: AppError | null; identityLabel: string | null; headDetached: boolean;
    onRefresh: () => void; onOpen: (id: string, side: FileSide, path: string) => void;
    onStage: (ids: string[]) => void; onUnstage: (ids: string[]) => void; onDiscard: (id: string) => void; onDiscardAll: () => void; onSubject: (value: string) => void; onBody: (value: string) => void; onCommit: () => void; onConflicts: () => void;
  } = $props();
  const changes = $derived(partitionStatus((files ?? []).filter(f => !f.conflicted)));
  const conflicts = $derived((files ?? []).filter(f => f.conflicted));
  const disabled = $derived(trustBlocked || indexBusy || commitBusy || loading || files === null);
  const commitHint = $derived(trustBlocked ? "Trust this repository to commit." : conflicts.length ? "Resolve conflicts before committing." : files === null ? "Load working changes first." : changes.staged.length === 0 ? "Stage a file to include it in your commit." : !subject.trim() ? "Write a summary for this commit." : `${changes.staged.length} staged ${changes.staged.length === 1 ? "file" : "files"} will be committed.`);
  const canCommit = $derived(!disabled && !conflicts.length && changes.staged.length > 0 && subject.trim() !== "");
  let fileMenu = $state<{ file: ChangedFile; side: FileSide; x: number; y: number } | null>(null);
  function commitKey(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault(); event.stopPropagation();
      if (canCommit) onCommit();
    }
  }
  function openFileMenu(event: MouseEvent | KeyboardEvent, file: ChangedFile, side: FileSide): void {
    event.preventDefault();
    event.stopPropagation();
    const point = pointFromContextEvent(event);
    fileMenu = { file, side, x:point.left, y:point.top };
  }
  let workContent: HTMLElement | undefined = $state(undefined);

  // Unstaged/Staged groups collapse via their headers; lists scroll past 250px.
  let collapsedGroups: Record<FileSide, boolean> = $state({ worktree: false, index: false });

  /** ArrowUp/Down (+Home/End) activate the neighbouring file row in visual
   * order, spanning Unstaged and Staged: focus moves and its diff opens,
   * exactly like clicking (or Enter on) the row. Only acts when focus is
   * already on a row; inputs in the commit editor keep their native keys. */
  function fileListKey(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    const row = target?.closest?.(".gd-file-row") as HTMLElement | null;
    if (!row || !workContent) return;
    const rows = [...workContent.querySelectorAll(".gd-file-row")];
    const next = neighborRowIndex(rows.length, rows.indexOf(row), event.key);
    if (next === null) return;
    event.preventDefault();
    const opener = rows[next].querySelector<HTMLElement>(".gd-file");
    if (!opener) return;
    opener.focus();
    opener.click();
  }

  function fileMenuItems(file: ChangedFile, side: FileSide): ContextMenuItem[] {
    const action = side === "worktree" ? onStage : onUnstage;
    const items: ContextMenuItem[] = [
      { id:"open", label:"Open diff", action:() => onOpen(file.pathId, side, file.displayPath) },
      { id:"index", label:side === "worktree" ? "Stage file" : "Unstage file", separatorBefore:true,
        disabled, action:() => action([file.pathId]) }
    ];
    if (side === "worktree") items.push(
      { id:"discard", label:"Discard file changes…", danger:true, disabled, action:() => onDiscard(file.pathId) }
    );
    items.push(
      { id:"copy-path", label:"Copy relative path", separatorBefore:true, action:() => navigator.clipboard.writeText(file.displayPath) }
    );
    if (file.originalDisplayPath) items.push({ id:"copy-original", label:"Copy original path", action:() => navigator.clipboard.writeText(file.originalDisplayPath ?? "") });
    return items;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions: keydown only delegates ArrowUp/Down/Home/End from an already-focused row button; the div itself takes no interaction -->
<div class="gd-work-content" bind:this={workContent} onkeydown={fileListKey}>
  <div class="gd-status-bar">
    <p class="gd-work-summary" title={branchName}><strong>{files === null ? "Reading changes…" : `${files.length} changed`}</strong><span>{branchName}</span>{#if loading && files !== null}<em>Refreshing…</em>{/if}</p>
    <button class="gd-refresh" onclick={onRefresh} disabled={loading} aria-label="Refresh working changes" title="Refresh working changes">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6 6a8 8 0 0 1 13 3M5 15a8 8 0 0 0 13 3"/></svg>
      <span>Refresh</span>
    </button>
  </div>
  {#if trustBlocked}<p class="gd-notice">Trust this repository to inspect and stage working changes.</p>
  {:else if error}<div class="gd-error" role="alert"><p>{error.message}</p><button class="gd-text-action" onclick={onRefresh}>Retry</button></div>{/if}
  {#if loading && files === null}<p class="gd-empty" role="status">Loading files…</p>
  {:else if files && !trustBlocked}
    {#if conflicts.length}<div class="gd-error"><strong>{conflicts.length} unresolved conflicts</strong><button class="gd-text-action" onclick={onConflicts}>Open conflicts →</button></div>{/if}
    {#each ["worktree", "index"] as source}
      {@const side = source as FileSide}
      {@const group = side === "worktree" ? changes.unstaged : changes.staged}
      {@const action = side === "worktree" ? "Stage" : "Unstage"}
      {@const groupLabel = side === "worktree" ? "Unstaged" : "Staged"}
      {@const collapsed = collapsedGroups[side]}
      <section class="gd-file-group" aria-label={groupLabel}>
        <div class="gd-group-heading">
          <button
            type="button"
            class="gd-group-toggle"
            aria-expanded={!collapsed}
            aria-label={`${collapsed ? "Expand" : "Collapse"} ${groupLabel.toLowerCase()} files`}
            title={`${collapsed ? "Expand" : "Collapse"} ${groupLabel.toLowerCase()} files`}
            onclick={() => (collapsedGroups = { ...collapsedGroups, [side]: !collapsedGroups[side] })}
          >
            <span class="gd-chevron" aria-hidden="true">{collapsed ? "▸" : "▾"}</span>
            <span class="gd-group-title">{groupLabel}<span class="gd-count">{group.length}</span></span>
          </button>
          <div class="gd-group-actions">
            {#if side === "worktree"}
              <button class="gd-text-action gd-danger-action" disabled={disabled || !group.length} onclick={onDiscardAll} aria-label="Discard all unstaged changes" title="Discard all unstaged changes">Discard all</button>
            {/if}
            <button class="gd-text-action" disabled={disabled || !group.length} onclick={() => (side === "worktree" ? onStage : onUnstage)(group.map(f => f.pathId))}>{action} all</button>
          </div>
        </div>
        {#if !collapsed}
        <div class="gd-group-list">
          {#if group.length}<ul>{#each group as file (file.pathId)}
            <FileChangeRow path={file.displayPath} oldPath={file.originalDisplayPath} status={side === "worktree" ? file.worktreeStatus : file.indexStatus}
              selected={selectedTarget?.kind === side && selectedTarget.pathId === file.pathId}
              contexted={fileMenu?.file.pathId === file.pathId && fileMenu.side === side}
              {disabled} {action}
              onOpen={() => onOpen(file.pathId, side, file.displayPath)}
              onAction={() => (side === "worktree" ? onStage : onUnstage)([file.pathId])}
              onDiscard={side === "worktree" ? () => onDiscard(file.pathId) : undefined}
              onContextMenu={(event) => openFileMenu(event, file, side)} />
          {/each}</ul>{:else}<p class="gd-empty">{side === "worktree" ? "No unstaged changes." : "Stage files with + to prepare your commit."}</p>{/if}
        </div>
        {/if}
      </section>
    {/each}
  {/if}
  {#if indexError}<p class="gd-error" role="alert">{indexError.message}</p>{/if}
  {#if headDetached}<p class="gd-notice">Detached HEAD. Create a branch to keep your next commit reachable.</p>{/if}
</div>
<footer class="gd-commit-editor">
  <div class="gd-editor-heading"><h3>Create commit</h3><span>Staged files only</span></div>
  <label for={summaryId}>Summary</label>
  <input id={summaryId} aria-label="Commit subject" placeholder="What changed?" value={subject} maxlength="500" oninput={e => onSubject(e.currentTarget.value)} onkeydown={commitKey} />
  <details open={body !== ""}><summary>Add description <span>optional</span></summary>
    <textarea aria-label="Commit body" placeholder="Why was this change needed?" value={body} rows="3" oninput={e => onBody(e.currentTarget.value)} onkeydown={commitKey}></textarea>
  </details>
  {#if identityLabel}<p class="gd-identity" title={identityLabel}>{identityLabel}</p>{/if}
  {#if commitError}<p class="gd-error" role="alert">{commitError.message} Your draft is saved.</p>{/if}
  <button class="gd-commit-button" disabled={!canCommit} title={commitHint} onclick={onCommit}>{commitBusy ? "Committing…" : `Commit ${changes.staged.length} ${changes.staged.length === 1 ? "file" : "files"}`}<span>⌃ ↵</span></button>
</footer>
{#if fileMenu}<ContextMenu x={fileMenu.x} y={fileMenu.y} items={fileMenuItems(fileMenu.file, fileMenu.side)}
  label={`File actions for ${fileMenu.file.displayPath}`} onClose={() => (fileMenu = null)} />{/if}

<style>
  .gd-work-content { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 8px 12px; }
  .gd-status-bar { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-height: 38px; margin-bottom: 14px; padding: 4px 4px 10px; border-bottom: 1px solid var(--gd-border); }
  .gd-refresh { display: flex; align-items: center; gap: 6px; flex: 0 0 auto; height: 28px; padding: 0 8px; border: 1px solid var(--gd-border); background: transparent; color: var(--gd-text-secondary); border-radius: 4px; cursor: pointer; font: 11px var(--gd-font-ui); }
  .gd-refresh:not(:disabled):hover { color: var(--gd-text); background: var(--gd-surface-hover); border-color: color-mix(in srgb, var(--gd-border) 60%, var(--gd-text-secondary)); }
  .gd-work-summary { display: flex; min-width: 0; flex-direction: column; gap: 2px; margin: 0; color: var(--gd-text-secondary); font-size: 10px; }
  .gd-work-summary strong { color: var(--gd-text); font-size: 12px; font-weight: 620; }
  .gd-work-summary span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-work-summary em { color: var(--gd-accent); font-style: normal; }
  .gd-file-group { margin-bottom: 14px; }
  .gd-group-heading { display: flex; align-items: center; justify-content: space-between; padding: 0 4px 8px; border-bottom: 1px solid var(--gd-border); margin-bottom: 4px; }
  .gd-group-toggle { display: flex; align-items: center; gap: 6px; margin: 0; padding: 2px 4px; background: transparent; border: 0; border-radius: 4px; cursor: pointer; color: var(--gd-text); font-size: 12px; font-weight: 600; }
  .gd-group-toggle:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 1px; }
  .gd-chevron { display: inline-block; width: 1.4ch; color: var(--gd-text-secondary); font-weight: 400; }
  .gd-group-title .gd-count { margin-left: 7px; padding: 1px 5px; background: var(--gd-surface-raised); color: var(--gd-text-secondary); border-radius: 3px; font-size: 10px; font-weight: 400; }
  .gd-editor-heading h3 { margin: 0; font-size: 12px; font-weight: 600; }
  .gd-group-list { height: 260px; overflow-y: auto; overflow-x: hidden; }
  ul { list-style: none; padding: 0; margin: 0; }
  .gd-group-actions { display: flex; align-items: center; gap: 8px; }
  .gd-text-action { border: 0; padding: 3px 4px; background: transparent; color: var(--gd-accent); font-size: 11px; cursor: pointer; }
  .gd-danger-action { color: var(--gd-danger); }
  button:disabled { opacity: .4; cursor: not-allowed; }
  .gd-text-action:not(:disabled):hover { text-decoration: underline; }
  .gd-empty { padding: 8px; color: var(--gd-text-secondary); font-size: 11px; line-height: 1.6; }
  .gd-clean { padding: 16px 6px; color: var(--gd-accent); }
  .gd-clean p { color: var(--gd-text-secondary); line-height: 1.6; font-size: 12px; }
  .gd-error, .gd-notice { padding: 10px; background: var(--gd-canvas); border-left: 2px solid var(--gd-danger); color: var(--gd-danger); font-size: 12px; line-height: 1.5; }
  .gd-error p { margin: 0; }
  .gd-notice { border-color: var(--gd-warning); color: var(--gd-warning); }
  .gd-commit-editor { flex: 0 0 auto; padding: 10px 12px; border-top: 1px solid var(--gd-border); background: color-mix(in srgb, var(--gd-panel) 65%, var(--gd-canvas)); }
  .gd-editor-heading { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
  .gd-editor-heading>span, .gd-commit-hint { color: var(--gd-text-secondary); font-size: 10px; }
  label { display: block; color: var(--gd-text-secondary); font-size: 11px; margin-bottom: 6px; }
  input, textarea { width: 100%; background: var(--gd-canvas); color: var(--gd-text); border: 1px solid var(--gd-border); border-radius: 4px; padding: 7px 8px; font: var(--gd-font-size-small)/1.4 var(--gd-font-ui); }
  textarea { resize: vertical; min-height: 64px; max-height: 140px; margin-top: 8px; }
  details { margin-top: 10px; font-size: 11px; color: var(--gd-text-secondary); }
  summary { cursor: pointer; }
  summary>span { opacity: .7; margin-left: 4px; }
  .gd-identity { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--gd-text-secondary); font-size: 10px; margin: 12px 0; }
  .gd-commit-button { width: 100%; display: flex; align-items: center; justify-content: center; position: relative; margin-top: 12px; padding: 9px; border: 1px solid transparent; border-radius: 4px; background: var(--gd-accent); color: var(--gd-on-accent); font-size: 12px; font-weight: 600; cursor: pointer; }
  .gd-commit-button>span { position: absolute; right: 10px; opacity: .65; font-size: 10px; }
  .gd-commit-hint { margin: 8px 0 0; line-height: 1.4; }
  button:focus-visible, input:focus-visible, textarea:focus-visible, summary:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
