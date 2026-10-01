<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { AppError, RecentEntry, RepoSnapshot } from "../ipc/types";
  import {
    clampPickerSelection,
    filterRecentRepos,
    formatLastOpened,
    movePickerSelection,
    repoBaseName
  } from "../repositories/picker";
  let { demo, recents, opened, busy, error, onOpen, onBrowse, onInit, onClone, onRemoveRecent, onClose }: {
    demo: boolean; recents: RecentEntry[]; opened: RepoSnapshot[]; busy: boolean; error: AppError | null;
    onOpen: (path: string) => void; onBrowse: () => void; onInit: () => void; onClone: () => void;
    onRemoveRecent: (entryId: string) => void; onClose: () => void;
  } = $props();
  let query = $state("");
  let selected = $state(0);
  let dialog: HTMLDivElement;
  let search: HTMLInputElement;
  const nowSecs = Math.floor(Date.now() / 1000);
  const filtered = $derived(filterRecentRepos(recents, query));
  const current = $derived(clampPickerSelection(selected, filtered.length));
  onMount(() => { search.focus(); });
  function reveal(index: number) {
    dialog.querySelector(`#gd-recent-option-${index}`)?.scrollIntoView({ block: "nearest" });
  }
  async function removeEntry(entryId: string): Promise<void> {
    if (busy) return;
    const focusInList = dialog.contains(document.activeElement) && document.activeElement !== search;
    onRemoveRecent(entryId);
    if (focusInList) {
      await tick();
      if (!dialog.contains(document.activeElement)) search.focus();
    }
  }
  function key(event: KeyboardEvent) {
    if (event.key === "Escape" && !busy) { event.preventDefault(); event.stopPropagation(); onClose(); return; }
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && !busy && filtered.length > 0) {
      event.preventDefault();
      selected = movePickerSelection(current, event.key === "ArrowDown" ? 1 : -1, filtered.length);
      reveal(selected);
      return;
    }
    if (event.key === "Enter" && !busy) {
      const target = event.target as HTMLElement | null;
      if (target === search || (target !== null && target.closest("button") === null)) {
        const entry = filtered[current];
        if (entry) { event.preventDefault(); onOpen(entry.displayPath); }
      }
      return;
    }
    if (event.key === "Delete" && !busy) {
      const target = event.target as HTMLElement | null;
      if (target !== null && target !== search && dialog.contains(target)) {
        const entry = filtered[current];
        if (entry) { event.preventDefault(); void removeEntry(entry.entryId); }
      }
      return;
    }
    if (event.key !== "Tab") return;
    const items = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)'));
    if (event.shiftKey && document.activeElement === items[0]) { event.preventDefault(); items.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0]?.focus(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: modal keyboard trap -->
<div class="gd-picker-backdrop" onkeydown={key} role="presentation">
  <div class="gd-repo-picker" role="dialog" aria-modal="true" aria-label="Open a repository" aria-busy={busy} bind:this={dialog}>
    <header><div><h2>Open a repository</h2><p>Keep your projects side by side. Each opens in its own tab.</p></div><button type="button" class="gd-dismiss" aria-label="Close repository picker" disabled={busy} onclick={onClose}>×</button></header>
    <input
      bind:this={search} type="search" bind:value={query} placeholder="Find a recent repository…" aria-label="Find recent repository"
      role="combobox" aria-expanded={filtered.length > 0} aria-controls="gd-recent-list"
      aria-activedescendant={filtered.length > 0 ? `gd-recent-option-${current}` : undefined}
      disabled={busy} oninput={() => { selected = 0; }} />
    <div class="gd-recent-heading">
      <span>Recent repositories{#if filtered.length > 0}<span class="gd-count">{filtered.length}</span>{/if}</span>
      {#if demo}<span class="gd-demo">Demo data</span>{/if}
    </div>
    <ul id="gd-recent-list" role="listbox" aria-label="Recent repositories">
      {#each filtered as repo, i (repo.entryId)}
        {@const name = repoBaseName(repo.displayPath)}
        {@const isOpen = opened.some(tab => tab.displayPath === repo.displayPath)}
        {@const openedAt = formatLastOpened(repo.lastOpenedAt, nowSecs)}
        <li role="option" id={`gd-recent-option-${i}`} aria-selected={i === current} class:gd-selected={i === current}>
          <button
            type="button" class="gd-open" disabled={busy}
            onfocus={() => { selected = i; }} onmouseenter={() => { selected = i; }}
            onclick={() => onOpen(repo.displayPath)} aria-label={`${isOpen ? "Switch to" : "Open"} ${repo.displayPath}`}>
            <svg class="gd-folder" viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" /></svg>
            <span class="gd-meta"><strong>{name}</strong><small>{repo.displayPath}</small></span>
            <span class="gd-side">
              <span class="gd-recent-action">{isOpen ? "Open tab ↗" : "Open →"}</span>
              {#if openedAt}<span class="gd-time">{openedAt}</span>{/if}
            </span>
          </button>
          <button
            type="button" class="gd-remove" disabled={busy}
            onfocus={() => { selected = i; }} onclick={() => void removeEntry(repo.entryId)}
            aria-label={`Remove ${repo.displayPath} from recents`} title="Remove from recents">×</button>
        </li>
      {:else}<li class="gd-empty" role="presentation">{query ? "No repositories match your search." : "No recent repositories. Choose a folder to get started."}</li>{/each}
    </ul>
    {#if error}<p class="gd-error" role="alert">{error.message}</p>{/if}
    {#if busy}<p class="gd-loading" role="status">Opening repository… Your current tabs will stay open.</p>{/if}
    <footer>
      <button type="button" class="gd-init" disabled={busy} onclick={onInit}>Initialize repository…</button>
      <button type="button" class="gd-init" disabled={busy} onclick={onClone}>Clone repository…</button>
      <button type="button" class="gd-browse" disabled={busy} onclick={onBrowse}>{demo ? "Open another demo" : "Browse folders…"}</button>
      <span class="gd-keys"><kbd>↑↓</kbd> navigate · <kbd>Enter</kbd> open · <kbd>Del</kbd> remove</span>
    </footer>
  </div>
</div>

<style>
  .gd-picker-backdrop { position: fixed; inset: 0; background: #0008; display: flex; justify-content: center; align-items: flex-start; padding-top: min(16vh,140px); z-index: 50; }
  .gd-repo-picker { width: 560px; max-width: calc(100vw - 48px); max-height: 75vh; overflow-y: auto; padding: var(--gd-space-5); border: 1px solid var(--gd-border); border-radius: var(--gd-radius-panel); background: var(--gd-panel); box-shadow: 0 20px 60px #0005; }
  header { display: flex; justify-content: space-between; gap: var(--gd-space-4); margin-bottom: var(--gd-space-4); }
  h2 { font-size: 18px; font-weight: 550; margin: 0 0 var(--gd-space-2); }
  header p { margin: 0; color: var(--gd-text-secondary); font-size: var(--gd-font-size); line-height: 1.5; }
  button { font: inherit; cursor: pointer; }
  button:disabled { opacity: .5; cursor: not-allowed; }
  .gd-dismiss { align-self: flex-start; background: transparent; border: 0; border-radius: var(--gd-radius-control); color: var(--gd-text-secondary); font-size: 22px; line-height: 1; padding: var(--gd-space-1) var(--gd-space-2); }
  .gd-dismiss:hover:not(:disabled) { background: var(--gd-surface-hover); color: var(--gd-text); }
  input { width: 100%; box-sizing: border-box; border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); padding: 10px 12px; background: var(--gd-canvas); color: var(--gd-text); font: var(--gd-font-size) var(--gd-font-ui); }
  .gd-recent-heading { display: flex; justify-content: space-between; align-items: center; margin: var(--gd-space-4) 0 var(--gd-space-2); font-size: var(--gd-font-size-small); color: var(--gd-text-secondary); }
  .gd-recent-heading > span:first-child { display: inline-flex; align-items: center; gap: var(--gd-space-2); }
  .gd-count { border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); color: var(--gd-accent); padding: 0 7px; line-height: 1.6; }
  .gd-demo { font-style: italic; }
  ul { list-style: none; padding: 0; margin: 0 calc(-1 * var(--gd-space-2)); max-height: 320px; overflow-y: auto; }
  li { display: flex; align-items: stretch; gap: var(--gd-space-1); padding: 0 var(--gd-space-2); border-radius: var(--gd-radius-control); }
  .gd-open { flex: 1; min-width: 0; display: flex; align-items: center; gap: var(--gd-space-3); padding: var(--gd-space-3) var(--gd-space-2); background: transparent; border: 0; border-radius: var(--gd-radius-control); color: var(--gd-text); text-align: left; }
  .gd-open:hover:not(:disabled) { background: var(--gd-surface-hover); }
  li.gd-selected > .gd-open:not(:disabled) { background: var(--gd-surface-selected); }
  .gd-folder { flex: none; color: var(--gd-text-secondary); }
  li.gd-selected > .gd-open > .gd-folder { color: var(--gd-accent); }
  .gd-meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: var(--gd-space-1); }
  strong { font-weight: 500; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { font-size: var(--gd-font-size-small); color: var(--gd-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-side { flex: none; display: flex; flex-direction: column; align-items: flex-end; gap: var(--gd-space-1); }
  .gd-recent-action { color: var(--gd-accent); font-size: var(--gd-font-size-small); white-space: nowrap; }
  .gd-time { font-size: var(--gd-font-size-small); color: var(--gd-text-secondary); white-space: nowrap; }
  .gd-remove { flex: none; align-self: center; background: transparent; border: 0; border-radius: var(--gd-radius-control); color: var(--gd-text-secondary); font-size: 18px; line-height: 1; padding: var(--gd-space-1) var(--gd-space-2); opacity: .65; }
  .gd-remove:hover:not(:disabled) { opacity: 1; color: var(--gd-danger); background: var(--gd-surface-hover); }
  .gd-empty, .gd-loading { padding: var(--gd-space-3) var(--gd-space-2); font-size: var(--gd-font-size); color: var(--gd-text-secondary); }
  .gd-error { padding: 10px; color: var(--gd-danger); border-left: 2px solid var(--gd-danger); font-size: var(--gd-font-size); }
  footer { margin-top: var(--gd-space-4); padding-top: var(--gd-space-4); border-top: 1px solid var(--gd-border); display: flex; flex-wrap: wrap; align-items: center; gap: var(--gd-space-3); }
  footer button { padding: var(--gd-space-2) var(--gd-space-3); border: 0; border-radius: var(--gd-radius-control); font-size: var(--gd-font-size); white-space: nowrap; }
  .gd-init { background: transparent; color: var(--gd-text-secondary); }
  .gd-init:hover:not(:disabled) { background: var(--gd-surface-hover); color: var(--gd-text); }
  .gd-browse { background: var(--gd-accent); color: var(--gd-on-accent); margin-left: auto; }
  .gd-keys { flex-basis: 100%; text-align: center; font-size: var(--gd-font-size-small); color: var(--gd-text-secondary); white-space: nowrap; }
  .gd-keys kbd { font-family: var(--gd-font-code); font-size: 10px; border: 1px solid var(--gd-border); border-radius: var(--gd-radius-control); padding: 0 var(--gd-space-1); background: var(--gd-canvas); }
  button:focus-visible, input:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
