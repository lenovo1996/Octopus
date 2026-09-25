<script lang="ts">
  import { tick } from "svelte";
  import { repositoryParent, type WorkspaceState } from "../repositories/tabs";
  import { headLabel } from "../../mocks/demoSession";
  import ContextMenu from "./ContextMenu.svelte";
  import { isContextMenuKey, pointFromContextEvent, type ContextMenuItem } from "../context-menu/model";
  let { tabs, activeId, opening, closingIds, onSelect, onClose, onAdd }: {
    tabs: WorkspaceState[]; activeId: string | null; opening: boolean; closingIds: string[];
    onSelect: (id: string) => void; onClose: (id: string) => void; onAdd: () => void;
  } = $props();
  let list: HTMLDivElement | undefined = $state();
  let tabMenu = $state<{ id: string; x: number; y: number } | null>(null);
  $effect(() => {
    activeId;
    void tick().then(() => list?.querySelector('[aria-selected="true"]')?.scrollIntoView({block:"nearest",inline:"nearest"}));
  });
  function tabKey(event: KeyboardEvent, id: string) {
    const index = tabs.findIndex(tab => tab.snapshot.repoId === id);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else if (event.key === "Delete") { event.preventDefault(); onClose(id); return; }
    else return;
    event.preventDefault();
    onSelect(tabs[next].snapshot.repoId);
    void tick().then(() => (list?.querySelector('[aria-selected="true"]') as HTMLElement | null)?.focus());
  }
  function openTabMenu(event: MouseEvent | KeyboardEvent, id: string): void {
    event.preventDefault();
    event.stopPropagation();
    const point = pointFromContextEvent(event);
    tabMenu = { id, x:point.left, y:point.top };
  }
  function tabMenuKey(event: KeyboardEvent, id: string): void {
    if (isContextMenuKey(event)) openTabMenu(event, id);
  }
  function menuItems(tab: WorkspaceState): ContextMenuItem[] {
    const repo = tab.snapshot;
    const closing = closingIds.includes(repo.repoId);
    return [
      { id:"switch", label:repo.repoId === activeId ? "Current repository" : "Switch to repository", disabled:repo.repoId === activeId,
        action:() => onSelect(repo.repoId) },
      { id:"copy-path", label:"Copy repository path", action:() => navigator.clipboard.writeText(repo.displayPath) },
      { id:"open-another", label:"Open another repository…", separatorBefore:true, disabled:opening, action:onAdd },
      { id:"close", label:"Close repository tab", hint:repo.repoId === activeId ? "Ctrl W" : undefined,
        disabled:tab.busy || closing || opening, separatorBefore:true, action:() => onClose(repo.repoId) }
    ];
  }
</script>

<nav class="gd-repositories" aria-label="Repository workspace">
  <span class="gd-brand" title="Open repositories"><img src="/brand/octopus-128.png" alt="" width="28" height="28" />Octopus<span>{tabs.length}</span></span>
  <div class="gd-tabs" role="tablist" aria-label="Open repositories" bind:this={list}>
    {#each tabs as tab (tab.snapshot.repoId)}
      {@const repo = tab.snapshot}
      {@const selected = repo.repoId === activeId}
      {@const closing = closingIds.includes(repo.repoId)}
      {@const duplicateName = tabs.some(other => other.snapshot.repoId !== repo.repoId && other.snapshot.displayName === repo.displayName)}
      <div class="gd-tab-wrap" class:selected class:contexted={tabMenu?.id === repo.repoId} role="presentation"
        oncontextmenu={(event) => openTabMenu(event, repo.repoId)} onkeydown={(event) => tabMenuKey(event, repo.repoId)}>
        <button class="gd-repo-tab" role="tab" id={`repo-tab-${repo.repoId}`} aria-controls={`repo-panel-${repo.repoId}`}
          aria-selected={selected} tabindex={selected ? 0 : -1} title={`${repo.displayPath}\n${headLabel(repo.head)}${tab.hasDraft ? "\nCommit draft saved" : ""}`}
          aria-label={`Repository ${repo.displayName}${duplicateName ? ` · ${repositoryParent(repo.displayPath)}` : ""}`}
          onclick={() => onSelect(repo.repoId)} onkeydown={e => tabKey(e, repo.repoId)}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M4 6h7l2 2h7v12H4zM4 6V4h7l2 2h7v2"/></svg>
          <span class="gd-tab-text"><strong>{repo.displayName}</strong><span>{duplicateName ? `${repositoryParent(repo.displayPath)} · ` : ""}{headLabel(repo.head)}</span></span>
          {#if tab.busy || closing}<span class="gd-tab-state busy" title="Git operation in progress" aria-label="Busy">···</span>
          {:else if tab.hasError}<span class="gd-tab-state error" title="This repository needs attention" aria-label="Needs attention">!</span>
          {:else if tab.hasDraft}<span class="gd-draft" title="Commit draft saved" aria-label="Commit draft saved"></span>
          {:else if tab.changedFiles}<span class="gd-tab-state" title={`${tab.changedFiles} changed files`}>{tab.changedFiles}</span>{/if}
        </button>
        <button class="gd-close-tab" aria-label={`Close repository ${repo.displayName}`} title={tab.busy ? "Wait for the Git operation to finish" : "Close tab · draft is saved"}
          disabled={tab.busy || closing || opening} tabindex={selected ? 0 : -1} onclick={() => onClose(repo.repoId)}>×</button>
      </div>
    {/each}
  </div>
  <button id="add-repository" class="gd-add-repo" onclick={onAdd} disabled={opening} aria-label="Add repository" title="Open another repository · Ctrl/⌘ O">
    <span aria-hidden="true">+</span>{opening ? "Opening…" : "Open repo"}
  </button>
  {#if tabMenu}
    {@const target = tabs.find(tab => tab.snapshot.repoId === tabMenu?.id)}
    {#if target}<ContextMenu x={tabMenu.x} y={tabMenu.y} items={menuItems(target)}
      label={`Repository actions for ${target.snapshot.displayName}`} onClose={() => (tabMenu = null)} />{/if}
  {/if}
</nav>

<style>
  .gd-repositories { flex: 0 0 49px; display: flex; align-items: stretch; background: var(--gd-canvas); border-bottom: 1px solid var(--gd-border); min-width: 0; }
  .gd-brand { display: flex; align-items: center; gap: 8px; padding: 0 16px; font-size: 12px; font-weight: 600; color: var(--gd-text-secondary); }
  .gd-brand>span { font: 10px var(--gd-font-code); opacity: .65; }
  .gd-brand img { flex: 0 0 auto; }
  .gd-tabs { display: flex; overflow-x: auto; min-width: 0; scrollbar-width: thin; flex: 1; }
  .gd-tab-wrap { display: flex; align-items: center; flex: 0 0 auto; max-width: 290px; min-width: 174px; border-right: 1px solid var(--gd-border); border-top: 2px solid transparent; padding-right: 7px; }
  .gd-tab-wrap.selected { background: var(--gd-panel); border-top-color: var(--gd-accent); }
  .gd-tab-wrap.contexted { background: var(--gd-surface-hover); box-shadow: inset 0 -2px var(--gd-focus); }
  .gd-repo-tab { display: flex; align-items: center; gap: 9px; flex: 1; min-width: 0; padding: 5px 9px 7px 12px; border: 0; background: transparent; color: var(--gd-text-secondary); text-align: left; cursor: pointer; }
  .selected .gd-repo-tab { color: var(--gd-text); }
  .selected svg { color: var(--gd-accent); }
  svg { flex: 0 0 auto; }
  .gd-tab-text { display: flex; flex-direction: column; min-width: 0; gap: 3px; }
  .gd-tab-text strong { font-size: 12px; font-weight: 550; }
  .gd-tab-text>span { color: var(--gd-text-secondary); font-size: 10px; }
  .gd-tab-text>* { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-close-tab { flex: 0 0 23px; height: 23px; border: 0; background: transparent; color: var(--gd-text-secondary); border-radius: 3px; font-size: 17px; cursor: pointer; }
  .gd-close-tab:hover:not(:disabled), .gd-add-repo:hover:not(:disabled), .gd-tab-wrap:hover:not(.selected) { background: var(--gd-surface-hover); }
  .gd-close-tab:disabled { opacity: .3; cursor: not-allowed; }
  .gd-add-repo { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; margin: 8px 12px; padding: 0 9px; border: 1px solid var(--gd-border); border-radius: 4px; background: transparent; color: var(--gd-text-secondary); font-size: 11px; cursor: pointer; }
  .gd-add-repo>span { font-size: 18px; color: var(--gd-accent); }
  .gd-tab-state { margin-left: auto; padding: 2px 4px; color: var(--gd-text-secondary); font: 10px var(--gd-font-code); }
  .gd-tab-state.busy { color: var(--gd-accent); font-weight: 700; }
  .gd-tab-state.error { color: var(--gd-danger); }
  .gd-draft { width: 5px; height: 5px; flex: 0 0 auto; border-radius: 50%; background: var(--gd-accent); margin-left: 4px; }
  button:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
</style>
