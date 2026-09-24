<script lang="ts">
  // Topbar: repo switcher, branch picker, search, identity popover, menu.
  // Open/init (T03) and history search (T06) are live; identity/menu (T14)
  // are read-only popovers over live IPC state. Primary Git actions render
  // centered via the `actions` snippet.
  import type { Snippet } from "svelte";

  interface Props {
    demo: boolean;
    branch: string;
    searchQuery: string;
    identityLabel: string | null;
    onSearchInput: (value: string) => void;
    onSearchFocus: () => void;
    onOpen: () => void;
    onInit: () => void;
    onClose: () => void;
    onBranches: () => void;
    onOpenSettings: () => void;
    onOpenHelp: () => void;
    repoName: string;
    actions?: Snippet;
  }

  let {
    demo,
    branch,
    searchQuery,
    identityLabel,
    onSearchInput,
    onSearchFocus,
    onOpen,
    onInit,
    onClose,
    onBranches,
    onOpenSettings,
    onOpenHelp,
    repoName,
    actions
  }: Props = $props();
  let searchEl: HTMLInputElement | undefined = $state(undefined);
  let showIdentity = $state(false);
  let showMenu = $state(false);

  export function focusSearch(): void {
    searchEl?.focus();
  }

  function popKeyDown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      showIdentity = false;
      showMenu = false;
    }
  }
</script>

<svelte:window onkeydown={popKeyDown} />

<header class="gd-topbar">
  <div class="gd-repo-nav">
    <span class="gd-repo-name" title="Open repository">{repoName}</span>
    <span class="gd-sep" aria-hidden="true"></span>
    <button type="button" class="gd-btn gd-branch" title="Current HEAD · manage branches" onclick={onBranches}>
      {branch} ▾
    </button>
  </div>
  <div class="gd-topbar-center">
    {#if actions}{@render actions()}{/if}
  </div>
  <div class="gd-topbar-right">
    <input
      bind:this={searchEl}
      type="search"
      class="gd-search"
      placeholder="Search history"
      aria-label="Search history"
      value={searchQuery}
      oninput={(e) => onSearchInput(e.currentTarget.value)}
      onkeydown={(e) => {
        if (e.key === "Escape" && searchQuery !== "") {
          onSearchInput("");
          e.currentTarget.blur();
        }
      }}
      onfocus={onSearchFocus}
    />
    <div class="gd-pop-wrap">
      <button
        type="button"
        class="gd-btn"
        aria-expanded={showIdentity}
        title="Commit identity for this repository (read-only)"
        onclick={() => {
          showIdentity = !showIdentity;
          showMenu = false;
        }}
      >
        Identity
      </button>
      {#if showIdentity}
        <div class="gd-pop" role="status">
          {identityLabel ?? "Open a repository to see its Git identity."}
        </div>
      {/if}
    </div>
    <div class="gd-pop-wrap">
      <button
        type="button"
        class="gd-btn"
        aria-expanded={showMenu}
        aria-label="Menu"
        title="Settings and help"
        onclick={() => {
          showMenu = !showMenu;
          showIdentity = false;
        }}
      >
        ☰
      </button>
      {#if showMenu}
        <div class="gd-pop gd-menu" role="menu">
          <button role="menuitem" onclick={() => { showMenu = false; onOpen(); }}>Open repository…</button>
          <button role="menuitem" onclick={() => { showMenu = false; onInit(); }}>Initialize repository…</button>
          <button role="menuitem" onclick={() => { showMenu = false; onClose(); }}>Close repository tab</button>
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              showMenu = false;
              onOpenSettings();
            }}
          >
            Settings…
          </button>
          <button
            type="button"
            role="menuitem"
            onclick={() => {
              showMenu = false;
              onOpenHelp();
            }}
          >
            Help &amp; shortcuts
          </button>
        </div>
      {/if}
    </div>
    {#if demo}
      <span class="gd-demo-badge">Demo data</span>
    {:else}
      <span class="gd-native-badge">Native</span>
    {/if}
  </div>
</header>

<style>
  .gd-topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--gd-space-3);
    height: var(--gd-topbar-height);
    padding: 0 var(--gd-space-3);
    background: var(--gd-panel);
    border-bottom: 1px solid var(--gd-border);
    flex: 0 0 auto;
  }
  .gd-repo-nav,
  .gd-topbar-right {
    display: flex;
    align-items: center;
    gap: var(--gd-space-2);
    min-width: 0;
    flex: 1 1 0;
  }
  .gd-topbar-right {
    justify-content: flex-end;
  }
  .gd-topbar-center {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 1 auto;
    min-width: 0;
    overflow-x: auto;
  }
  .gd-btn {
    padding: 5px 10px;
    color: var(--gd-text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    font-size: var(--gd-font-size);
    white-space: nowrap;
  }
  .gd-btn:hover {
    background: var(--gd-surface-hover);
  }
  .gd-btn:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-pop-wrap {
    position: relative;
  }
  .gd-pop {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    min-width: 240px;
    max-width: 360px;
    background: var(--gd-panel);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    padding: var(--gd-space-2) var(--gd-space-3);
    font-size: var(--gd-font-size-small);
    z-index: 30;
  }
  .gd-menu {
    display: flex;
    flex-direction: column;
    padding: var(--gd-space-1);
    min-width: 180px;
  }
  .gd-menu button {
    background: transparent;
    border: none;
    color: var(--gd-text);
    cursor: pointer;
    padding: 6px 8px;
    text-align: left;
    border-radius: var(--gd-radius-control);
  }
  .gd-menu button:hover {
    background: var(--gd-surface-selected);
  }
  .gd-branch {
    border-color: var(--gd-border);
  }
  .gd-repo-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }
  .gd-btn:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  .gd-sep {
    width: 1px;
    height: 20px;
    background: var(--gd-border);
  }
  .gd-search {
    width: 220px;
    padding: 5px 10px;
    color: var(--gd-text);
    background: var(--gd-canvas);
    border: 1px solid var(--gd-border);
    border-radius: var(--gd-radius-control);
    font-size: var(--gd-font-size);
  }
  .gd-search:focus {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-demo-badge,
  .gd-native-badge {
    font-size: var(--gd-font-size-small);
    padding: 2px 8px;
    border-radius: var(--gd-radius-control);
    border: 1px solid var(--gd-border);
    white-space: nowrap;
  }
  .gd-demo-badge {
    color: var(--gd-warning);
    border-color: var(--gd-warning);
  }
  .gd-native-badge {
    color: var(--gd-accent);
    border-color: var(--gd-accent);
  }
</style>
