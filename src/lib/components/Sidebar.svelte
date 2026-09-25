<script lang="ts">
  // RepositorySidebar: working changes entry + local/remote refs and tags.
  // Ref rows carry a filter box per section plus a right-click (or
  // Context Menu key) menu wired to the real branch flows: switch, track,
  // show-in-graph, copy helpers and safe delete. No placeholder buttons.
  // P2 placeholders (FR-23/24): PRs "Planned", submodules read-only summary.
  // Stash listing has no read IPC in this milestone, so the section carries
  // an explanation instead of a fake count.
  import type { RefItem } from "../ipc/types";
  import ContextMenu from "./ContextMenu.svelte";
  import {
    isContextMenuKey,
    pointFromContextEvent,
    type ContextMenuItem
  } from "../context-menu/model";
  import { filterRefs, isRefsSectionOpen } from "../refs/filter";
  import { loadStarred, saveStarred, sortStarred } from "../refs/starred";
  import { windowRows } from "../graph/layout";
  import {
    buildBranchMenuItems,
    type BranchMenuAction
  } from "../refs/branch-menu";

  interface Props {
    width: number;
    refs: RefItem[];
    /** Stable per-repository scope for starred-branch persistence. */
    starScope: string;
    activeSection: string;
    activeRefId: string | null;
    actionsDisabled: boolean;
    selectedCommitOid: string | null;
    onSelect: (section: string) => void;
    onRefSelect: (refId: string) => void;
    onBranchAction: (action: BranchMenuAction, ref: RefItem) => void;
  }

  let {
    width,
    refs,
    starScope,
    activeSection,
    activeRefId,
    actionsDisabled,
    selectedCommitOid,
    onSelect,
    onRefSelect,
    onBranchAction
  }: Props = $props();

  const localRefs = $derived(refs.filter((r) => r.kind === "local"));
  const remoteRefs = $derived(refs.filter((r) => r.kind === "remote"));
  const tagRefs = $derived(refs.filter((r) => r.kind === "tag"));

  // One shared search box filters Local, Remote and Tags together.
  let query = $state("");
  let refMenu = $state<{ ref: RefItem; x: number; y: number } | null>(null);

  // Local/Remote panels collapse via their headers; a filter reopens them.
  let collapsedLocal = $state(false);
  let collapsedRemote = $state(false);

  // Starred branches pin to the top of Local/Remote; persisted per repo.
  let starred: string[] = $state([]);
  $effect(() => {
    starred = loadStarred(starScope);
  });
  const starredSet = $derived(new Set(starred));

  function toggleStar(refId: string): void {
    const next = starred.includes(refId)
      ? starred.filter((id) => id !== refId)
      : [...starred, refId];
    starred = next;
    saveStarred(starScope, next);
  }

  const shownLocal = $derived(sortStarred(filterRefs(localRefs, query), starredSet));
  const shownRemote = $derived(sortStarred(filterRefs(remoteRefs, query), starredSet));
  const shownTag = $derived(filterRefs(tagRefs, query));
  const filtering = $derived(query.trim() !== "");

  // Long branch lists render only the visible window (fixed 28px rows, same
  // windowing helper as the history graph) so thousands of refs stay fast.
  const REF_ROW_HEIGHT = 28;
  const REF_OVERSCAN = 8;
  const REF_VIEWPORT = 240;
  let localList: HTMLElement | undefined = $state(undefined);
  let remoteList: HTMLElement | undefined = $state(undefined);
  let localScrollTop = $state(0);
  let remoteScrollTop = $state(0);
  // A new filter or collapse changes the listing: restart from the top.
  $effect(() => {
    query;
    collapsedLocal;
    if (localList) localList.scrollTop = 0;
    localScrollTop = 0;
  });
  $effect(() => {
    query;
    collapsedRemote;
    if (remoteList) remoteList.scrollTop = 0;
    remoteScrollTop = 0;
  });
  const localWindow = $derived(
    windowRows(shownLocal, localScrollTop, REF_VIEWPORT, REF_ROW_HEIGHT, REF_OVERSCAN)
  );
  const remoteWindow = $derived(
    windowRows(shownRemote, remoteScrollTop, REF_VIEWPORT, REF_ROW_HEIGHT, REF_OVERSCAN)
  );

  function refTitle(ref: RefItem): string {
    const parts = [ref.fullName, ref.oid.slice(0, 7)];
    if (ref.current) parts.push("checked out");
    if (ref.checkedOutElsewhere) parts.push("checked out in another worktree");
    return parts.join(" · ");
  }

  function openRefMenu(event: MouseEvent | KeyboardEvent, ref: RefItem): void {
    event.preventDefault();
    event.stopPropagation();
    const point = pointFromContextEvent(event);
    refMenu = { ref, x: point.left, y: point.top };
  }

  function refMenuKey(event: KeyboardEvent, ref: RefItem): void {
    if (isContextMenuKey(event)) openRefMenu(event, ref);
  }

  function menuItems(ref: RefItem): ContextMenuItem[] {
    return buildBranchMenuItems(
      ref,
      { actionsDisabled, selectedCommitOid },
      (action, target) => onBranchAction(action, target),
      (text) => {
        void navigator.clipboard.writeText(text);
      }
    );
  }
</script>

{#snippet refRow(ref: RefItem, starrable: boolean)}
  {@const isStarred = starrable && starredSet.has(ref.refId)}
  <li class="gd-ref-row">
    <span class="gd-ref-icon" class:starred={isStarred}>
      <span class="gd-kind" aria-hidden="true">{ref.kind === "local" ? "⑂" : ref.kind === "remote" ? "☁" : "⚑"}</span>
      {#if starrable}
        <button
          type="button"
          class="gd-star"
          aria-pressed={isStarred}
          aria-label={`${isStarred ? "Unstar" : "Star"} branch ${ref.label}`}
          title={isStarred ? `Unstar ${ref.label}` : `Star ${ref.label} to pin it to the top`}
          onclick={(event) => {
            event.stopPropagation();
            toggleStar(ref.refId);
          }}
        >
          <span aria-hidden="true">{isStarred ? "★" : "☆"}</span>
        </button>
      {/if}
    </span>
    <button
      type="button"
      class="gd-ref"
      class:active={activeRefId === ref.refId}
      class:contexted={refMenu?.ref.refId === ref.refId}
      title={refTitle(ref)}
      onclick={() => onRefSelect(ref.refId)}
      oncontextmenu={(event) => openRefMenu(event, ref)}
      onkeydown={(event) => refMenuKey(event, ref)}
    >
      <span class="gd-ref-label">{ref.label}{ref.current ? " •" : ""}</span>
    </button>
  </li>
{/snippet}

<aside class="gd-sidebar" style="width: {width}px" aria-label="Repository" onscroll={() => (refMenu = null)}>
  <input
    type="search"
    class="gd-ref-search"
    placeholder="Search branches"
    aria-label="Search branches"
    value={query}
    oninput={(e) => (query = e.currentTarget.value)}
    onkeydown={(e) => {
      if (e.key === "Escape") query = "";
    }}
  />
  <nav>
    <ul>
      <li>
        <button
          type="button"
          class:active={activeSection === "local"}
          onclick={() => { onSelect("local"); collapsedLocal = !collapsedLocal; }}
          title="Local branches: toggle panel (selects the section to scope the history)"
          aria-expanded={isRefsSectionOpen(collapsedLocal, filtering)}
        >
          <span class="gd-section-label"><span class="gd-chevron" aria-hidden="true">{collapsedLocal && !filtering ? "▸" : "▾"}</span>Local</span>
          {#if localRefs.length > 0}
            <span class="gd-count">
              {filtering ? `${shownLocal.length} of ${localRefs.length}` : localRefs.length}
            </span>
          {/if}
        </button>
        {#if isRefsSectionOpen(collapsedLocal, filtering)}
        <ul class="gd-refs" bind:this={localList} onscroll={(e) => (localScrollTop = e.currentTarget.scrollTop)}>
          <li class="gd-spacer" aria-hidden="true" style={`height: ${localWindow.start * REF_ROW_HEIGHT}px`}></li>
          {#each shownLocal.slice(localWindow.start, localWindow.end) as ref (ref.refId)}
            {@render refRow(ref, true)}
          {:else}
            <li>
              <span class="gd-ref gd-empty">
                {filtering ? "No matching branches" : "No local branches"}
              </span>
            </li>
          {/each}
          <li class="gd-spacer" aria-hidden="true" style={`height: ${(shownLocal.length - localWindow.end) * REF_ROW_HEIGHT}px`}></li>
        </ul>
        {/if}
      </li>
      <li>
        <button
          type="button"
          class:active={activeSection === "remote"}
          onclick={() => { onSelect("remote"); collapsedRemote = !collapsedRemote; }}
          title="Remote tracking branches: toggle panel (selects the section to scope the history)"
          aria-expanded={isRefsSectionOpen(collapsedRemote, filtering)}
        >
          <span class="gd-section-label"><span class="gd-chevron" aria-hidden="true">{collapsedRemote && !filtering ? "▸" : "▾"}</span>Remote</span>
          {#if remoteRefs.length > 0}
            <span class="gd-count">
              {filtering ? `${shownRemote.length} of ${remoteRefs.length}` : remoteRefs.length}
            </span>
          {/if}
        </button>
        {#if isRefsSectionOpen(collapsedRemote, filtering)}
        <ul class="gd-refs" bind:this={remoteList} onscroll={(e) => (remoteScrollTop = e.currentTarget.scrollTop)}>
          <li class="gd-spacer" aria-hidden="true" style={`height: ${remoteWindow.start * REF_ROW_HEIGHT}px`}></li>
          {#each shownRemote.slice(remoteWindow.start, remoteWindow.end) as ref (ref.refId)}
            {@render refRow(ref, true)}
          {:else}
            <li>
              <span class="gd-ref gd-empty">
                {filtering ? "No matching branches" : "No remote branches"}
              </span>
            </li>
          {/each}
          <li class="gd-spacer" aria-hidden="true" style={`height: ${(shownRemote.length - remoteWindow.end) * REF_ROW_HEIGHT}px`}></li>
        </ul>
        {/if}
      </li>
      <li>
        <button
          type="button"
          class:active={activeSection === "tags"}
          onclick={() => onSelect("tags")}
          title="Tags: select one to scope the history"
        >
          <span class="gd-section-label">Tags</span>
          {#if tagRefs.length > 0}
            <span class="gd-count">
              {filtering ? `${shownTag.length} of ${tagRefs.length}` : tagRefs.length}
            </span>
          {/if}
        </button>
        {#if activeSection === "tags"}
          <ul class="gd-refs">
            {#each shownTag as ref (ref.refId)}
              {@render refRow(ref, false)}
            {:else}
              <li>
                <span class="gd-ref gd-empty">
                  {filtering ? "No matching tags" : "No tags"}
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
      <li>
        <button
          type="button"
          class:active={activeSection === "stashes"}
          onclick={() => onSelect("stashes")}
          title="Stash listing is not part of this milestone"
        >
          <span class="gd-section-label">Stashes</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class:active={activeSection === "prs"}
          onclick={() => onSelect("prs")}
          title="Planned after MVP (FR-23): provider PR integration"
        >
          <span class="gd-section-label">Pull requests</span>
          <span class="gd-tag">Planned</span>
        </button>
      </li>
      <li>
        <button
          type="button"
          class:active={activeSection === "submodules"}
          onclick={() => onSelect("submodules")}
          title="Read-only in MVP (FR-24): submodule summary only"
        >
          <span class="gd-section-label">Submodules</span>
        </button>
      </li>
    </ul>
  </nav>
  {#if activeSection === "stashes"}
    <p class="gd-note">Stash entries are not listed in this milestone. Stash apply/restore arrives with the worktree operations.</p>
  {:else if activeSection === "prs"}
    <p class="gd-note">Pull requests are planned after MVP. No provider data is shown.</p>
  {:else if activeSection === "submodules"}
    <p class="gd-note">Submodules are read-only in MVP: summary only, no init/update controls.</p>
  {/if}
  {#if refMenu}
    <ContextMenu
      x={refMenu.x}
      y={refMenu.y}
      items={menuItems(refMenu.ref)}
      label={`Branch actions for ${refMenu.ref.label}`}
      onClose={() => (refMenu = null)}
    />
  {/if}
</aside>

<style>
  .gd-sidebar {
    flex: 0 0 auto;
    min-height: 0;
    min-width: 0;
    overflow-y: auto;
    overflow-x: hidden;
    background: var(--gd-panel);
    padding: var(--gd-space-2);
  }
  .gd-sidebar ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .gd-sidebar button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 5px 8px;
    color: var(--gd-text);
    background: transparent;
    border: 0;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    font-size: var(--gd-font-size);
  }
  .gd-sidebar button:hover {
    background: var(--gd-surface-hover);
  }
  .gd-sidebar button.active {
    background: var(--gd-surface-selected);
  }
  .gd-sidebar button:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-count {
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
  }
  .gd-tag {
    font-size: var(--gd-font-size-small);
    color: var(--gd-warning);
    border: 1px solid var(--gd-warning);
    border-radius: var(--gd-radius-control);
    padding: 0 6px;
  }
  .gd-refs {
    margin-left: var(--gd-space-4);
    max-height: 240px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .gd-ref-row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    overflow: hidden;
  }
  .gd-ref-row .gd-ref {
    flex: 1;
    min-width: 0;
  }
  .gd-ref-label {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gd-spacer {
    list-style: none;
    padding: 0;
  }
  .gd-ref-icon {
    position: relative;
    flex: 0 0 1.2em;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .gd-star {
    position: absolute;
    inset: -4px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--gd-warning);
    background: transparent;
    border: 0;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    font-size: var(--gd-font-size-small);
    opacity: 0;
  }
  .gd-ref-icon.starred .gd-kind,
  .gd-ref-row:hover .gd-kind {
    opacity: 0;
  }
  .gd-ref-icon.starred .gd-star,
  .gd-ref-row:hover .gd-star,
  .gd-star:focus-visible {
    opacity: 1;
  }
  .gd-star:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-chevron {
    display: inline-block;
    width: 1.4ch;
    color: var(--gd-text-secondary);
  }
  .gd-ref {
    display: flex !important;
    justify-content: flex-start !important;
    gap: 6px;
    padding: 4px 10px !important;
    font-size: var(--gd-font-size-small) !important;
    color: var(--gd-text-secondary) !important;
  }
  .gd-ref.gd-empty {
    cursor: default;
  }
  .gd-ref.gd-empty:hover {
    background: transparent;
  }
  .gd-ref.contexted {
    background: var(--gd-surface-hover);
  }
  .gd-ref-search {
    width: 100%;
    margin: 0 0 var(--gd-space-1);
    padding: 3px 6px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    font-size: var(--gd-font-size-small);
  }
  .gd-ref-search::placeholder {
    color: var(--gd-text-secondary);
  }
  .gd-note {
    padding: var(--gd-space-2) var(--gd-space-3);
    font-size: var(--gd-font-size-small);
    color: var(--gd-text-secondary);
  }
</style>
