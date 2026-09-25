<script lang="ts">
  import type { AppError, CommitDetails, CommitFileChange, DiffTarget, RefItem } from "../ipc/types";
  import { refBadge } from "../history/refs";
  import FileChangeRow from "./FileChangeRow.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import { pointFromContextEvent, type ContextMenuItem } from "../context-menu/model";
  let { details, loading, error, refs, selectedTarget, onOpen, onParentChange, onParentCommit, onRetry }: {
    details: CommitDetails | null; loading: boolean; error: AppError | null; refs: RefItem[]; selectedTarget: DiffTarget | null;
    onOpen: (id: string) => void; onParentChange: (index: number | null) => void; onParentCommit: (oid: string) => void; onRetry: () => void; onBack: () => void;
  } = $props();
  let query = $state("");
  let copyState = $state("");
  let fileMenu = $state<{ file: CommitFileChange; x: number; y: number } | null>(null);
  const files = $derived(details?.files.filter(f => `${f.path}\n${f.oldPath ?? ""}`.toLowerCase().includes(query.toLowerCase())) ?? []);
  const badges = $derived(refs.filter(r => r.oid === details?.oid).map(refBadge));
  $effect(() => { details?.oid; query = ""; copyState = ""; });
  function formattedDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(date);
  }
  async function copyOid(): Promise<void> {
    const oid = details?.oid;
    if (!oid) return;
    try {
      await navigator.clipboard.writeText(oid);
      if (details?.oid === oid) copyState = "Copied";
    } catch { if (details?.oid === oid) copyState = "Select the hash below to copy it."; }
  }
  function openFileMenu(event: MouseEvent | KeyboardEvent, file: CommitFileChange): void {
    event.preventDefault();
    event.stopPropagation();
    const point = pointFromContextEvent(event);
    fileMenu = { file, x:point.left, y:point.top };
  }
  function fileMenuItems(file: CommitFileChange): ContextMenuItem[] {
    const items: ContextMenuItem[] = [
      { id:"open", label:"Open diff", action:() => onOpen(file.pathId) },
      { id:"copy-path", label:"Copy relative path", separatorBefore:true, action:() => navigator.clipboard.writeText(file.path) }
    ];
    if (file.oldPath) items.push({ id:"copy-original", label:"Copy original path", action:() => navigator.clipboard.writeText(file.oldPath ?? "") });
    return items;
  }
</script>

<div class="gd-details-content">
  {#if loading}<p class="gd-empty" role="status">Loading commit details…</p>
  {:else if error}<div class="gd-error" role="alert"><p>{error.message}</p><button class="gd-text-action" onclick={onRetry}>Retry</button></div>
  {:else if details}
    <div class="gd-detail-heading"><span class="gd-eyebrow">Commit</span><button class="gd-copy" onclick={() => void copyOid()} aria-label="Copy commit ID">{copyState === "Copied" ? "Copied" : "Copy ID"}
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V4H4v12h4"/></svg>
    </button></div>
    <h2>{details.subject}</h2>
    <code class="gd-oid" title={details.oid}>{details.oid}</code>
    {#if copyState}<span class="gd-copy-status" role="status">{copyState}</span>{/if}
    {#if badges.length}<div class="gd-commit-refs">{#each badges as badge (badge.id)}<span class:remote={badge.kind === "remote"} title={badge.fullName}>{badge.source}<strong>{badge.name}</strong></span>{/each}</div>{/if}
    <div class="gd-author-card"><span class="gd-avatar" aria-hidden="true">{details.authorName.trim().slice(0, 1).toUpperCase() || "?"}</span>
      <div><strong>{details.authorName}</strong><time datetime={details.committedAt} title={details.committedAt}>{formattedDate(details.committedAt)}</time></div>
    </div>
    {#if details.body}<p class="gd-message">{details.body}</p>{/if}
    <section class="gd-comparison" aria-label="Parent comparison">
      <div class="gd-section-heading"><h3>Compare changes</h3><span>{details.parents.length > 1 ? "Merge commit" : details.parents.length ? "1 parent" : "Root commit"}</span></div>
      {#if details.parents.length > 1}
        <select value={details.parentIndex ?? 0} onchange={e => onParentChange(Number(e.currentTarget.value))} aria-label="Compare to parent">
          {#each details.parents as parent, i (parent)}<option value={i}>Parent {i + 1} · {parent.slice(0, 8)}</option>{/each}
        </select>
      {:else if details.parents.length === 0}<p class="gd-muted">Compared with an empty tree.</p>{/if}
      {#if details.parents.length}
        {@const parent = details.parents[details.parentIndex ?? 0]}
        <button class="gd-parent-link" onclick={() => onParentCommit(parent)}>View parent <code>{parent.slice(0, 8)}</code><span>→</span></button>
      {/if}
    </section>
    <section class="gd-files" aria-label="Changed files">
      <div class="gd-section-heading"><h3>Changed files <span class="gd-count">{details.files.length}</span></h3><span>Click to view diff</span></div>
      {#if details.files.length}
        <input class="gd-file-search" type="search" aria-label="Filter changed files" placeholder="Filter files…" bind:value={query} />
        {#if files.length}<ul>{#each files as file (file.pathId)}
          <FileChangeRow path={file.path} oldPath={file.oldPath} status={file.status}
            selected={selectedTarget?.kind === "commit" && selectedTarget.pathId === file.pathId}
            contexted={fileMenu?.file.pathId === file.pathId}
            onOpen={() => onOpen(file.pathId)} onContextMenu={(event) => openFileMenu(event, file)} />
        {/each}</ul>{:else}<p class="gd-empty">No files match “{query}”.</p>{/if}
      {:else}<p class="gd-empty">No file changes for this comparison.</p>{/if}
    </section>
  {:else}<div class="gd-empty"><strong>Explore a commit</strong><p>Select a row in history to see its message, author and changed files.</p></div>{/if}
</div>
{#if fileMenu}<ContextMenu x={fileMenu.x} y={fileMenu.y} items={fileMenuItems(fileMenu.file)}
  label={`File actions for ${fileMenu.file.path}`} onClose={() => (fileMenu = null)} />{/if}

<style>
  .gd-details-content { flex: 1; min-height: 0; padding: 16px; overflow-y: auto; }
  .gd-detail-heading { display: flex; justify-content: space-between; align-items: center; }
  .gd-eyebrow { font-size: 11px; color: var(--gd-text-secondary); }
  .gd-copy { display: flex; align-items: center; gap: 6px; background: transparent; color: var(--gd-text-secondary); border: 1px solid var(--gd-border); border-radius: 4px; padding: 5px 7px; font-size: 10px; cursor: pointer; }
  h2 { font-size: 18px; font-weight: 550; line-height: 1.4; margin: 14px 0 10px; overflow-wrap: anywhere; }
  .gd-oid { display: block; color: var(--gd-text-secondary); font: 10px/1.5 var(--gd-font-code); overflow-wrap: anywhere; user-select: all; }
  .gd-copy-status { display: block; color: var(--gd-accent); font-size: 10px; margin-top: 5px; }
  .gd-commit-refs { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 12px; }
  .gd-commit-refs>span { padding: 3px 6px; border: 1px solid var(--gd-border); border-radius: 3px; font-size: 10px; color: var(--gd-accent); overflow-wrap: anywhere; }
  .gd-commit-refs .remote { color: var(--gd-lane-2); }
  .gd-commit-refs strong { margin-left: 6px; font-weight: 500; }
  .gd-author-card { display: flex; gap: 10px; align-items: center; margin: 20px 0; }
  .gd-avatar { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 6px; background: var(--gd-surface-raised); color: var(--gd-accent); font-size: 12px; }
  .gd-author-card>div { display: flex; flex-direction: column; min-width: 0; gap: 3px; }
  .gd-author-card strong { font-size: 12px; font-weight: 500; overflow-wrap: anywhere; }
  time { color: var(--gd-text-secondary); font-size: 10px; }
  .gd-message { font-size: 12px; color: var(--gd-text-secondary); line-height: 1.65; white-space: pre-wrap; overflow-wrap: anywhere; margin: 0 0 20px; }
  .gd-comparison { padding: 14px 0; border-top: 1px solid var(--gd-border); border-bottom: 1px solid var(--gd-border); margin-bottom: 20px; }
  .gd-section-heading { display: flex; align-items: center; justify-content: space-between; gap: 6px; margin-bottom: 10px; }
  h3 { margin: 0; font-size: 12px; font-weight: 600; }
  .gd-section-heading>span, .gd-muted { font-size: 10px; color: var(--gd-text-secondary); }
  .gd-count { margin-left: 5px; color: var(--gd-text-secondary); }
  select, input { width: 100%; border: 1px solid var(--gd-border); border-radius: 4px; background: var(--gd-canvas); color: var(--gd-text); font: 12px var(--gd-font-ui); padding: 8px; }
  .gd-parent-link { display: flex; align-items: center; gap: 7px; width: 100%; padding: 8px 0 0; background: transparent; border: 0; font-size: 11px; color: var(--gd-text-secondary); cursor: pointer; }
  .gd-parent-link code { color: var(--gd-accent); font-family: var(--gd-font-code); }
  .gd-parent-link>span { margin-left: auto; }
  .gd-file-search { margin-bottom: 8px; }
  ul { list-style: none; margin: 0 -6px; padding: 0; }
  .gd-empty { padding: 12px 0; color: var(--gd-text-secondary); font-size: 12px; line-height: 1.6; }
  .gd-details-footer { padding: 12px 16px; border-top: 1px solid var(--gd-border); }
  .gd-details-footer button, .gd-text-action { background: transparent; border: 0; color: var(--gd-accent); font-size: 12px; padding: 4px 0; cursor: pointer; }
  .gd-error { color: var(--gd-danger); font-size: 12px; line-height: 1.5; }
  button:focus-visible, select:focus-visible, input:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
  button:hover { color: var(--gd-accent); }
</style>
