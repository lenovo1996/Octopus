<script lang="ts">
  import { onMount } from "svelte";
  import type { AppError, RecentEntry, RepoSnapshot } from "../ipc/types";
  let { demo, recents, opened, busy, error, onOpen, onBrowse, onInit, onClose }: {
    demo: boolean; recents: RecentEntry[]; opened: RepoSnapshot[]; busy: boolean; error: AppError | null;
    onOpen: (path: string) => void; onBrowse: () => void; onInit: () => void; onClose: () => void;
  } = $props();
  let query = $state("");
  let dialog: HTMLDivElement;
  let search: HTMLInputElement;
  const filtered = $derived(recents.filter(repo => repo.displayPath.toLowerCase().includes(query.toLowerCase())));
  onMount(() => { search.focus(); });
  function key(event: KeyboardEvent) {
    if (event.key === "Escape" && !busy) { event.preventDefault(); event.stopPropagation(); onClose(); }
    if (event.key !== "Tab") return;
    const items = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled)'));
    if (event.shiftKey && document.activeElement === items[0]) { event.preventDefault(); items.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0]?.focus(); }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: modal keyboard trap -->
<div class="gd-picker-backdrop" onkeydown={key} role="presentation">
  <div class="gd-repo-picker" role="dialog" aria-modal="true" aria-label="Open a repository" bind:this={dialog}>
    <header><div><h2>Open a repository</h2><p>Keep your projects side by side. Each opens in its own tab.</p></div><button class="gd-dismiss" aria-label="Close repository picker" disabled={busy} onclick={onClose}>×</button></header>
    <input bind:this={search} type="search" bind:value={query} placeholder="Find a recent repository…" aria-label="Find recent repository" />
    <div class="gd-recent-heading"><span>Recent repositories</span>{#if demo}<span>Demo data</span>{/if}</div>
    <ul>
      {#each filtered as repo (repo.entryId)}
        {@const name = repo.displayPath.replace(/\\/g,"/").split("/").filter(Boolean).at(-1) ?? repo.displayPath}
        {@const isOpen = opened.some(tab => tab.displayPath === repo.displayPath)}
        <li><button disabled={busy} onclick={() => onOpen(repo.displayPath)} aria-label={`${isOpen ? "Switch to" : "Open"} ${repo.displayPath}`}>
          <span><strong>{name}</strong><small>{repo.displayPath}</small></span><span class="gd-recent-action">{isOpen ? "Open tab ↗" : "Open →"}</span>
        </button></li>
      {:else}<li class="gd-empty">{query ? "No repositories match your search." : "No recent repositories. Choose a folder to get started."}</li>{/each}
    </ul>
    {#if error}<p class="gd-error" role="alert">{error.message}</p>{/if}
    {#if busy}<p class="gd-loading" role="status">Opening repository… Your current tabs will stay open.</p>{/if}
    <footer><button class="gd-init" disabled={busy} onclick={onInit}>Initialize repository…</button><button class="gd-browse" disabled={busy} onclick={onBrowse}>{demo ? "Open another demo" : "Browse folders…"}</button></footer>
  </div>
</div>

<style>
  .gd-picker-backdrop { position: fixed; inset: 0; background: #0008; display: flex; justify-content: center; align-items: flex-start; padding-top: min(16vh,140px); z-index: 50; }
  .gd-repo-picker { width: 560px; max-height: 75vh; overflow-y: auto; padding: 24px; border: 1px solid var(--gd-border); border-radius: 8px; background: var(--gd-panel); box-shadow: 0 20px 60px #0005; }
  header { display: flex; justify-content: space-between; gap: 16px; margin-bottom: 20px; }
  h2 { font-size: 18px; font-weight: 550; margin: 0 0 8px; }
  header p { margin: 0; color: var(--gd-text-secondary); font-size: 12px; line-height: 1.5; }
  button { font: inherit; cursor: pointer; }
  button:disabled { opacity: .5; cursor: not-allowed; }
  .gd-dismiss { align-self: flex-start; background: transparent; border: 0; color: var(--gd-text-secondary); font-size: 22px; }
  input { width: 100%; border: 1px solid var(--gd-border); border-radius: 4px; padding: 10px 12px; background: var(--gd-canvas); color: var(--gd-text); font: 12px var(--gd-font-ui); }
  .gd-recent-heading { display: flex; justify-content: space-between; margin: 20px 0 8px; font-size: 11px; color: var(--gd-text-secondary); }
  ul { list-style: none; padding: 0; margin: 0 -8px; }
  li button { width: 100%; display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 12px 8px; background: transparent; border: 0; border-radius: 4px; color: var(--gd-text); text-align: left; }
  li button:hover { background: var(--gd-surface-hover); }
  li button>span:first-child { display: flex; flex-direction: column; min-width: 0; gap: 5px; }
  strong { font-weight: 500; font-size: 13px; }
  small { font-size: 11px; color: var(--gd-text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-recent-action { color: var(--gd-accent); font-size: 11px; white-space: nowrap; }
  .gd-empty, .gd-loading { padding: 12px 8px; font-size: 12px; color: var(--gd-text-secondary); }
  .gd-error { padding: 10px; color: var(--gd-danger); border-left: 2px solid var(--gd-danger); font-size: 12px; }
  footer { margin-top: 20px; padding-top: 16px; border-top: 1px solid var(--gd-border); display: flex; justify-content: space-between; }
  footer button { padding: 8px 12px; border: 0; border-radius: 4px; font-size: 12px; }
  .gd-init { background: transparent; color: var(--gd-text-secondary); }
  .gd-browse { background: var(--gd-accent); color: var(--gd-on-accent); }
  button:focus-visible, input:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
