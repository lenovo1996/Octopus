<script lang="ts">
  import { isContextMenuKey } from "../context-menu/model";
  let { path, oldPath = null, status, selected = false, contexted = false, disabled = false, action = null,
    onOpen, onAction = () => {}, onDiscard = undefined, onContextMenu = undefined }: {
    path: string; oldPath?: string | null; status: string; selected?: boolean; disabled?: boolean;
    contexted?: boolean; action?: "Stage" | "Unstage" | null; onOpen: () => void; onAction?: () => void; onDiscard?: () => void;
    onContextMenu?: (event: MouseEvent | KeyboardEvent) => void;
  } = $props();
  const name = $derived(path.slice(path.lastIndexOf("/") + 1));
  const directory = $derived(path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "Repository root");
  const statusName = $derived(({M:"Modified",A:"Added",D:"Deleted",R:"Renamed",C:"Copied",T:"Type changed",U:"Conflicted","?":"Untracked"} as Record<string,string>)[status] ?? status);
  function contextKey(event: KeyboardEvent): void {
    if (onContextMenu && isContextMenuKey(event)) onContextMenu(event);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: right-click supplements the row's existing buttons -->
<li class:selected class:contexted class="gd-file-row" oncontextmenu={onContextMenu} onkeydown={contextKey}>
  <span class="gd-file-status" class:deleted={status === "D"} class:added={status === "A" || status === "?"} title={statusName}>{status}</span>
  <button type="button" class="gd-file" aria-label={path} aria-current={selected ? "true" : undefined} title={oldPath ? `${oldPath} → ${path}` : path} onclick={onOpen}>
    <span class="gd-file-name">{name}</span><span class="gd-file-directory">{oldPath ? `← ${oldPath}` : directory}</span>
  </button>
  <div class="gd-file-actions">
    {#if onDiscard}<button class="gd-file-action gd-discard" type="button" {disabled} aria-label={`Discard changes in ${path}`} title="Discard file changes" onclick={onDiscard}>↶</button>{/if}
    {#if action}<button class="gd-file-action gd-index" type="button" {disabled} aria-label={`${action} ${path}`} title={`${action} file`} onclick={onAction}>{action === "Stage" ? "+" : "−"}</button>{/if}
  </div>
</li>

<style>
  .gd-file-row { display: flex; align-items: center; gap: 9px; min-height: 48px; padding: 5px 8px; border-left: 2px solid transparent; border-radius: 4px; }
  .gd-file-row:hover { background: var(--gd-surface-hover); }
  .gd-file-row.selected { background: var(--gd-surface-selected); border-left-color: var(--gd-accent); }
  .gd-file-row.contexted { background: var(--gd-surface-hover); box-shadow: inset 3px 0 var(--gd-focus); }
  .gd-file-status { flex: 0 0 22px; display: grid; place-items: center; width: 22px; height: 22px; font: 650 10px var(--gd-font-code); color: var(--gd-warning); background: color-mix(in srgb, currentColor 10%, transparent); border-radius: 4px; }
  .gd-file-status.added { color: var(--gd-accent); }
  .gd-file-status.deleted { color: var(--gd-danger); }
  .gd-file { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; background: transparent; color: var(--gd-text); border: 0; text-align: left; padding: 2px 0; cursor: pointer; font: inherit; }
  .gd-file-name, .gd-file-directory { display: block; width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .gd-file-name { font-size: var(--gd-font-size-small); }
  .gd-file-directory { color: var(--gd-text-secondary); font-size: 10px; line-height: 14px; }
  .gd-file-actions { flex: 0 0 auto; display: flex; gap: 4px; }
  .gd-file-action { flex: 0 0 27px; width: 27px; height: 27px; border: 1px solid transparent; border-radius: 4px; color: var(--gd-text-secondary); background: transparent; font-size: 16px; cursor: pointer; opacity: .72; }
  .gd-file-row:hover .gd-file-action, .gd-file-row:focus-within .gd-file-action { opacity: 1; background: var(--gd-canvas); border-color: var(--gd-border); }
  .gd-index:hover { color: var(--gd-accent); border-color: var(--gd-accent) !important; }
  .gd-discard:hover { color: var(--gd-danger); border-color: var(--gd-danger) !important; }
  .gd-file-action:disabled { opacity: .4; cursor: not-allowed; }
  button:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: 2px; }
</style>
