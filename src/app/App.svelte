<script lang="ts">
  import { onMount, tick } from "svelte";
  import RepositoryWorkspace from "./RepositoryWorkspace.svelte";
  import RepositoryTabs from "../lib/components/RepositoryTabs.svelte";
  import RepositoryPicker from "../lib/components/RepositoryPicker.svelte";
  import Welcome from "../lib/components/Welcome.svelte";
  import InitModal from "../lib/components/InitModal.svelte";
  import { isNative, newRequestId, normalizeTransportError } from "../lib/ipc/client";
  import { mockAdapter, createMockAdapter } from "../lib/ipc/mock";
  import { realAdapter } from "../lib/ipc/real";
  import type { AppError, PreflightData, RecentEntry, RepoSnapshot } from "../lib/ipc/types";
  import { activeWorkspaceKey, initialWorkspace, openWorkspaceEntries, resolveRestoredActive, type WorkspaceState } from "../lib/repositories/tabs";
  import { demoSession } from "../mocks/demoSession";

  interface RepositoryTab extends WorkspaceState {
    initialSession: RepoSnapshot;
    adapter: typeof mockAdapter;
  }
  const demo = !isNative();
  let tabs: RepositoryTab[] = $state([]);
  let activeId: string | null = $state(null);
  let opening = $state(false);
  let closingIds: string[] = $state([]);
  let error: AppError | null = $state(null);
  let pickerOpen = $state(false);
  let showInit = $state(false);
  let initFolder = $state("");
  let initBranch = $state("main");
  let preflight: PreflightData | null = $state(null);
  let preflightError: AppError | null = $state(null);
  let recents: RecentEntry[] = $state([]);
  const demoRepositories = new Map<string, typeof mockAdapter>();
  let demoIndex = 0;
  const currentTab = $derived(tabs.find(tab => tab.snapshot.repoId === activeId));
  // Workspace restore (T19): set once the launch restore settles so the
  // save effect below never persists an empty tab list over saved state.
  let restored = $state(false);
  let restoring = $state(false);

  function appError(value: unknown): AppError {
    if (value && typeof value === "object" && "code" in value && "message" in value) return value as AppError;
    return normalizeTransportError(value, newRequestId());
  }
  async function loadPreflight() {
    try { preflight = demo ? await mockAdapter.appPreflight({requestId:newRequestId()}) : await realAdapter.appPreflight(); preflightError = null; }
    catch (e) { preflightError = appError(e); }
  }
  async function loadRecents() {
    try { recents = demo ? await mockAdapter.repoRecentList() : await realAdapter.repoRecentList(); }
    catch (e) { error = appError(e); }
  }
  onMount(() => {
    void loadPreflight();
    void loadRecents();
    if (demo) restored = true;
    else void restoreWorkspaces();
  });

  // Persist tab order + active tab (debounced). Skipped before the launch
  // restore settles and in browser demo mode.
  $effect(() => {
    if (demo || !restored || restoring) return;
    const entries = openWorkspaceEntries(tabs);
    const activeKey = activeWorkspaceKey(tabs, activeId);
    const timer = setTimeout(() => {
      void realAdapter.workspacesSave(entries, activeKey).catch(() => {});
    }, 400);
    return () => clearTimeout(timer);
  });

  async function restoreWorkspaces() {
    restoring = true;
    try {
      const result = await realAdapter.workspacesRestore();
      for (const snapshot of result.opened) attach(snapshot, mockAdapter);
      const next = resolveRestoredActive(result.opened, result.activeKey);
      if (next) activeId = next;
      if (result.skipped.length > 0) {
        const names = result.skipped.map(skip => skip.displayPath).join(", ");
        error = { code: "REPO_UNAVAILABLE", message: `${result.skipped.length} saved workspace(s) could not be reopened and were forgotten: ${names}. Reopen them from Recents if they moved.`, recovery: "chooseRepository", retryable: false };
      }
      await loadRecents();
    } catch (e) {
      error = appError(e);
    } finally {
      restoring = false;
      restored = true;
    }
  }

  function demoRepository(path: string) {
    let adapter = demoRepositories.get(path);
    if (!adapter) {
      adapter = createMockAdapter({ ...demoSession, repoId: `demo:${path}`, workspaceKey: `demo-worktree:${path}`, displayPath: path, displayName: path.split("/").filter(Boolean).at(-1) ?? path });
      demoRepositories.set(path, adapter);
    }
    return adapter;
  }
  function attach(snapshot: RepoSnapshot, adapter: typeof mockAdapter) {
    const existing = tabs.find(tab => tab.snapshot.repoId === snapshot.repoId);
    if (!existing) tabs = [...tabs, {...initialWorkspace(snapshot), initialSession: snapshot, adapter}];
    activeId = snapshot.repoId;
    pickerOpen = false;
    error = null;
  }
  async function openPaths(paths: string[]) {
    if (opening || !paths.length) return;
    opening = true;
    error = null;
    try {
      for (const path of paths) {
        const existing = tabs.find(tab => tab.snapshot.displayPath === path);
        if (existing) { activeId = existing.snapshot.repoId; pickerOpen = false; continue; }
        const adapter = demo ? demoRepository(path) : mockAdapter;
        const snapshot = demo ? await adapter.repoOpen(path) : await realAdapter.repoOpen(path);
        attach(snapshot, adapter);
      }
      await loadRecents();
    } catch (e) { error = appError(e); }
    finally { opening = false; }
  }
  async function browse() {
    if (opening) return;
    if (demo) {
      const path = demoIndex++ === 0 && !tabs.length ? "/demo/gitdock-demo" : `/demo/project-${demoIndex}`;
      await openPaths([path]);
      return;
    }
    // The picker is part of the guarded open flow; a cancelled or failed picker leaves tabs intact.
    opening = true;
    error = null;
    let paths: string[] = [];
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const result = await open({ directory: true, multiple: true, title: "Open repositories" });
      paths = typeof result === "string" ? [result] : result ?? [];
    } catch (e) { error = appError(e); }
    finally { opening = false; }
    await openPaths(paths);
  }
  function showPicker() { if (!opening) { error = null; pickerOpen = true; void loadRecents(); } }
  function closePicker() {
    pickerOpen = false;
    void tick().then(() => document.getElementById("add-repository")?.focus());
  }
  function selectTab(id: string) {
    if (tabs.some(tab => tab.snapshot.repoId === id)) { activeId = id; error = null; }
  }
  async function closeTab(id: string) {
    if (opening) return;
    const tab = tabs.find(item => item.snapshot.repoId === id);
    if (!tab || closingIds.includes(id)) return;
    if (tab.busy) { error = {code:"REPO_BUSY",message:`${tab.snapshot.displayName} is still running a Git operation. Wait for it to finish before closing.`,recovery:"inspectState",retryable:false}; return; }
    closingIds = [...closingIds, id];
    error = null;
    try {
      if (demo) await tab.adapter.repoClose();
      else await realAdapter.repoClose(id);
      const index = tabs.findIndex(item => item.snapshot.repoId === id);
      tabs = tabs.filter(item => item.snapshot.repoId !== id);
      if (activeId === id) activeId = tabs[Math.min(index, tabs.length - 1)]?.snapshot.repoId ?? null;
      await loadRecents();
    } catch (e) { error = appError(e); }
    finally { closingIds = closingIds.filter(closing => closing !== id); }
  }
  function updateWorkspace(id: string, state: WorkspaceState) {
    tabs = tabs.map(tab => tab.snapshot.repoId === id ? {...tab, ...state} : tab);
  }
  async function startInit() {
    if (opening) return;
    error = null;
    opening = true;
    try {
      if (demo) initFolder = `/demo/new-repository-${++demoIndex}`;
      else {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const folder = await open({ directory:true, multiple:false, title:"Initialize repository" });
        if (typeof folder !== "string") return;
        initFolder = folder;
      }
      initBranch = "main";
      pickerOpen = false;
      showInit = true;
    } catch (e) { error = appError(e); }
    finally { opening = false; }
  }
  async function confirmInit() {
    if (opening) return;
    opening = true;
    error = null;
    try {
      const adapter = demo ? demoRepository(initFolder) : mockAdapter;
      const snapshot = demo ? await adapter.repoInit(initFolder, initBranch.trim()) : await realAdapter.repoInit(initFolder, initBranch.trim());
      attach(snapshot, adapter);
      showInit = false;
      await loadRecents();
    } catch (e) { error = appError(e); }
    finally { opening = false; }
  }
  async function removeRecent(id: string) {
    try {
      if (demo) { recents = recents.filter(entry => entry.entryId !== id); return; }
      await realAdapter.repoRecentRemove(id); await loadRecents();
    } catch (e) { error = appError(e); }
  }
  function shortcuts(event: KeyboardEvent) {
    if (event.defaultPrevented || pickerOpen || showInit || currentTab?.modalOpen || !(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (key === "tab" && tabs.length) {
      event.preventDefault();
      const index = tabs.findIndex(tab => tab.snapshot.repoId === activeId);
      selectTab(tabs[(index + (event.shiftKey ? -1 : 1) + tabs.length) % tabs.length].snapshot.repoId);
    } else if (key === "o") { event.preventDefault(); showPicker(); }
    else if (key === "w" && activeId) { event.preventDefault(); void closeTab(activeId); }
  }
</script>

<svelte:window onkeydown={shortcuts} />
<div class="gd-app">
  {#if tabs.length}
    <div inert={pickerOpen || showInit || currentTab?.modalOpen || false}>
      <RepositoryTabs {tabs} {activeId} {closingIds} {opening} onSelect={selectTab} onClose={id => void closeTab(id)} onAdd={showPicker} />
    </div>
    {#each tabs as tab (tab.snapshot.repoId)}
      <div class="gd-workspace" role="tabpanel" id={`repo-panel-${tab.snapshot.repoId}`} aria-labelledby={`repo-tab-${tab.snapshot.repoId}`}
        hidden={tab.snapshot.repoId !== activeId} inert={tab.snapshot.repoId !== activeId || pickerOpen || showInit || closingIds.includes(tab.snapshot.repoId)}>
        <RepositoryWorkspace initialSession={tab.initialSession} active={tab.snapshot.repoId === activeId && !pickerOpen && !showInit}
          mockAdapter={tab.adapter} onOpenRepository={showPicker} onInitRepository={() => void startInit()}
          onCloseRepository={() => void closeTab(tab.snapshot.repoId)} onWorkspaceChange={state => updateWorkspace(tab.snapshot.repoId,state)} />
      </div>
    {/each}
    {#if error && !pickerOpen && !showInit}<div class="gd-open-error" role="alert"><span>{error.message}</span><button aria-label="Dismiss repository error" onclick={() => (error = null)}>×</button></div>{/if}
  {:else}
    {#if restoring}
      <div class="gd-restore" role="status">Restoring workspaces…</div>
    {:else}
      <Welcome {demo} {preflight} {preflightError} {recents} busy={opening} {error} onOpen={() => void browse()} onInit={() => void startInit()}
        onOpenRecent={path => void openPaths([path])} onRemoveRecent={id => void removeRecent(id)} onRetryPreflight={() => void loadPreflight()} />
    {/if}
  {/if}
  {#if pickerOpen}<RepositoryPicker {demo} {recents} opened={tabs.map(tab=>tab.snapshot)} busy={opening} {error}
    onOpen={path=>void openPaths([path])} onBrowse={()=>void browse()} onInit={()=>void startInit()} onClose={closePicker} />{/if}
  {#if showInit}<InitModal folder={initFolder} branch={initBranch} busy={opening} {error} onBranch={value=>(initBranch=value)} onConfirm={()=>void confirmInit()} onCancel={()=>(showInit=false)} />{/if}
</div>

<style>
  .gd-app { display: flex; flex-direction: column; height: 100dvh; min-width: 1100px; overflow: hidden; background: var(--gd-canvas); color: var(--gd-text); font: var(--gd-font-size) var(--gd-font-ui); }
  .gd-workspace { flex: 1; min-height: 0; }
  .gd-workspace[hidden] { display: none; }
  .gd-open-error { display: flex; align-items: center; justify-content: space-between; gap: 16px; color: var(--gd-danger); background: var(--gd-panel); padding: 8px 16px; font-size: 12px; border-top: 1px solid var(--gd-border); }
  .gd-open-error button { background: transparent; border: 0; color: inherit; font-size: 20px; cursor: pointer; }
  .gd-restore { display: flex; align-items: center; justify-content: center; height: 100dvh; color: var(--gd-text-secondary); font-size: 13px; }
</style>
