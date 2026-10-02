<script lang="ts">
  import { onMount, tick } from "svelte";
  import RepositoryWorkspace from "./RepositoryWorkspace.svelte";
  import RepositoryTabs from "../lib/components/RepositoryTabs.svelte";
  import RepositoryAliasModal from "../lib/components/RepositoryAliasModal.svelte";
  import RepositoryPicker from "../lib/components/RepositoryPicker.svelte";
  import Welcome from "../lib/components/Welcome.svelte";
  import InitModal from "../lib/components/InitModal.svelte";
  import CloneModal from "../lib/components/CloneModal.svelte";
  import ToastCenter from "../lib/components/ToastCenter.svelte";
  import { pushToast } from "../lib/toast";
  import { isNative, newRequestId, normalizeTransportError } from "../lib/ipc/client";
  import { mockAdapter, createMockAdapter } from "../lib/ipc/mock";
  import { realAdapter } from "../lib/ipc/real";
  import type { AppError, PreflightData, RecentEntry, RepoSnapshot } from "../lib/ipc/types";
  import { activeWorkspaceKey, initialWorkspace, openWorkspaceEntries, reorderTabs, resolveRestoredActive, type WorkspaceState } from "../lib/repositories/tabs";
  import { demoSession } from "../mocks/demoSession";
  import { aliasValidationError } from "../lib/repositories/alias";

  interface RepositoryTab extends WorkspaceState {
    initialSession: RepoSnapshot;
    adapter: typeof mockAdapter;
  }
  // Optional adapter lets browser fixtures exercise the real app's alias error flow.
  let { aliasCommands }: { aliasCommands?: Pick<typeof realAdapter, "repoAliasGet" | "repoAliasSet"> } = $props();
  const demo = !isNative();
  let tabs: RepositoryTab[] = $state([]);
  let activeId: string | null = $state(null);
  let opening = $state(false);
  let closingIds: string[] = $state([]);
  let error: AppError | null = $state(null);
  let pickerOpen = $state(false);
  let showInit = $state(false);
  let showClone = $state(false);
  let cloneUrl = $state("");
  let cloneFolder = $state("");
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
  let aliasTargetId: string | null = $state(null);
  let aliasDraft = $state("");
  let aliasLoading = $state(false);
  let aliasLoaded = $state(false);
  let aliasBusy = $state(false);
  let aliasError: AppError | null = $state(null);
  const aliasTarget = $derived(tabs.find(tab => tab.snapshot.repoId === aliasTargetId));
  const aliasReads = new Map<string, number>();
  let aliasSequence = 0;
  let aliasDialogRead = 0;

  async function readAlias(id: string): Promise<string | null | undefined> {
    const tab = tabs.find(item => item.snapshot.repoId === id);
    if (!tab) return;
    const token = ++aliasSequence;
    aliasReads.set(id, token);
    const result = await (aliasCommands ?? (demo ? tab.adapter : realAdapter)).repoAliasGet(id);
    if (aliasReads.get(id) !== token || result.workspaceKey !== tab.snapshot.workspaceKey) return;
    tabs = tabs.map(item => item.snapshot.repoId === id ? { ...item, alias: result.alias } : item);
    return result.alias;
  }
  async function loadAliasDialog() {
    const id = aliasTargetId;
    if (!id) return;
    const generation = ++aliasDialogRead;
    aliasLoading = true;
    aliasLoaded = false;
    aliasError = null;
    try {
      const alias = await readAlias(id);
      if (aliasTargetId !== id || generation !== aliasDialogRead || alias === undefined) return;
      aliasDraft = alias ?? "";
      aliasLoaded = true;
    } catch (e) { if (aliasTargetId === id && generation === aliasDialogRead) aliasError = appError(e); }
    finally { if (aliasTargetId === id && generation === aliasDialogRead) aliasLoading = false; }
  }
  function openAlias(id: string) {
    if (opening || closingIds.includes(id) || !tabs.some(tab => tab.snapshot.repoId === id)) return;
    aliasTargetId = id;
    aliasDraft = "";
    void loadAliasDialog();
  }
  function closeAlias() {
    if (aliasBusy) return;
    const id = aliasTargetId;
    if (id) aliasReads.delete(id);
    aliasDialogRead += 1;
    aliasTargetId = null;
    void tick().then(() => document.getElementById(`repo-tab-${id}`)?.focus());
  }
  async function saveAlias() {
    const tab = aliasTarget;
    if (!tab || aliasBusy || aliasLoading || !aliasLoaded || aliasValidationError(aliasDraft)) return;
    aliasBusy = true;
    aliasError = null;
    aliasReads.set(tab.snapshot.repoId, ++aliasSequence);
    try {
      const result = await (aliasCommands ?? (demo ? tab.adapter : realAdapter)).repoAliasSet(tab.snapshot.repoId, aliasDraft.trim() || null);
      if (result.workspaceKey !== tab.snapshot.workspaceKey) throw new Error("Repository changed; reopen the alias dialog.");
      tabs = tabs.map(item => item.snapshot.repoId === tab.snapshot.repoId ? { ...item, alias: result.alias } : item);
      aliasBusy = false;
      closeAlias();
    } catch (e) { aliasError = appError(e); }
    finally { aliasBusy = false; }
  }

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
    if (!existing) {
      tabs = [...tabs, {...initialWorkspace(snapshot), alias: null, initialSession: snapshot, adapter}];
      void readAlias(snapshot.repoId).catch(e => { if (tabs.some(tab => tab.snapshot.repoId === snapshot.repoId)) error = appError(e); });
    }
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
      const path = demoIndex++ === 0 && !tabs.length ? "/demo/octopus-demo" : `/demo/project-${demoIndex}`;
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
      aliasReads.delete(id);
      if (activeId === id) activeId = tabs[Math.min(index, tabs.length - 1)]?.snapshot.repoId ?? null;
      await loadRecents();
    } catch (e) { error = appError(e); }
    finally { closingIds = closingIds.filter(closing => closing !== id); }
  }
  function updateWorkspace(id: string, state: WorkspaceState) {
    tabs = tabs.map(tab => tab.snapshot.repoId === id ? {...tab, ...state} : tab);
  }
  function moveTabById(fromId: string, toId: string, before: boolean) {
    tabs = reorderTabs(tabs, fromId, toId, before);
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
      pushToast("success", `Initialized repository in ${initFolder}`);
      await loadRecents();
    } catch (e) { error = appError(e); }
    finally { opening = false; }
  }
  function startClone() {
    if (opening) return;
    error = null;
    cloneUrl = "";
    cloneFolder = demo ? `/demo/cloned-${++demoIndex}` : "";
    pickerOpen = false;
    showClone = true;
  }
  async function browseCloneFolder() {
    if (opening) return;
    if (demo) { cloneFolder = `/demo/cloned-${++demoIndex}`; return; }
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const folder = await open({ directory:true, multiple:false, title:"Choose clone destination" });
      if (typeof folder === "string") cloneFolder = folder;
    } catch (e) { error = appError(e); }
  }
  async function confirmClone() {
    if (opening) return;
    opening = true;
    error = null;
    try {
      const adapter = demo ? demoRepository(cloneFolder) : mockAdapter;
      const snapshot = demo ? await adapter.repoClone(cloneUrl.trim(), cloneFolder) : await realAdapter.repoClone(cloneUrl.trim(), cloneFolder);
      attach(snapshot, adapter);
      showClone = false;
      pushToast("success", `Cloned ${cloneUrl.trim()} to ${cloneFolder}`);
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
    if (event.defaultPrevented || pickerOpen || showInit || showClone || aliasTargetId !== null || currentTab?.modalOpen || !(event.ctrlKey || event.metaKey)) return;
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
    <div inert={pickerOpen || showInit || showClone || aliasTargetId !== null || currentTab?.modalOpen || false}>
      <RepositoryTabs {tabs} {activeId} {closingIds} {opening} onSelect={selectTab} onClose={id => void closeTab(id)} onAdd={showPicker} onMove={moveTabById} onAlias={openAlias} />
    </div>
    {#each tabs as tab (tab.snapshot.repoId)}
      <div class="gd-workspace" role="tabpanel" id={`repo-panel-${tab.snapshot.repoId}`} aria-labelledby={`repo-tab-${tab.snapshot.repoId}`}
        hidden={tab.snapshot.repoId !== activeId} inert={tab.snapshot.repoId !== activeId || pickerOpen || showInit || showClone || aliasTargetId !== null || closingIds.includes(tab.snapshot.repoId)}>
        <RepositoryWorkspace initialSession={tab.initialSession} active={tab.snapshot.repoId === activeId && !pickerOpen && !showInit && !showClone && aliasTargetId === null}
          mockAdapter={tab.adapter} onOpenRepository={showPicker} onInitRepository={() => void startInit()}
          onCloseRepository={() => void closeTab(tab.snapshot.repoId)} onWorkspaceChange={state => updateWorkspace(tab.snapshot.repoId,state)} />
      </div>
    {/each}
    {#if error && !pickerOpen && !showInit && !showClone}<div class="gd-open-error" role="alert"><span>{error.message}</span><button aria-label="Dismiss repository error" onclick={() => (error = null)}>×</button></div>{/if}
  {:else}
    {#if restoring}
      <div class="gd-restore" role="status">Restoring workspaces…</div>
    {:else}
      <Welcome {demo} {preflight} {preflightError} {recents} busy={opening} {error} onOpen={() => void browse()} onClone={startClone} onInit={() => void startInit()}
        onOpenRecent={path => void openPaths([path])} onRemoveRecent={id => void removeRecent(id)} onRetryPreflight={() => void loadPreflight()} />
    {/if}
  {/if}
  {#if pickerOpen}<RepositoryPicker {demo} {recents} opened={tabs.map(tab=>tab.snapshot)} busy={opening} {error}
    onOpen={path=>void openPaths([path])} onBrowse={()=>void browse()} onInit={()=>void startInit()} onClone={startClone} onRemoveRecent={id=>void removeRecent(id)} onClose={closePicker} />{/if}
  {#if showInit}<InitModal folder={initFolder} branch={initBranch} busy={opening} {error} onBranch={value=>(initBranch=value)} onConfirm={()=>void confirmInit()} onCancel={()=>(showInit=false)} />{/if}
  {#if showClone}<CloneModal sourceUrl={cloneUrl} folder={cloneFolder} busy={opening} {error} onUrl={value=>(cloneUrl=value)} onBrowseFolder={()=>void browseCloneFolder()} onConfirm={()=>void confirmClone()} onCancel={()=>(showClone=false)} />{/if}
  {#if aliasTarget}<RepositoryAliasModal name={aliasTarget.snapshot.displayName} path={aliasTarget.snapshot.displayPath}
    value={aliasDraft} loading={aliasLoading} loaded={aliasLoaded} busy={aliasBusy} error={aliasError}
    onChange={value => { aliasDraft = value; aliasError = null; }} onSave={() => void saveAlias()} onClose={closeAlias} onRetry={() => void loadAliasDialog()} />{/if}
  <ToastCenter />
</div>

<style>
  .gd-app { display: flex; flex-direction: column; height: 100dvh; min-width: 1100px; overflow: hidden; background: var(--gd-canvas); color: var(--gd-text); font: var(--gd-font-size) var(--gd-font-ui); }
  .gd-workspace { flex: 1; min-height: 0; }
  .gd-workspace[hidden] { display: none; }
  .gd-open-error { display: flex; align-items: center; justify-content: space-between; gap: 16px; color: var(--gd-danger); background: var(--gd-panel); padding: 8px 16px; font-size: 12px; border-top: 1px solid var(--gd-border); }
  .gd-open-error button { background: transparent; border: 0; color: inherit; font-size: 20px; cursor: pointer; }
  .gd-restore { display: flex; align-items: center; justify-content: center; height: 100dvh; color: var(--gd-text-secondary); font-size: 13px; }
</style>
