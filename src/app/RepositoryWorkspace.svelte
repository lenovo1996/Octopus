<script lang="ts">
  // GitDock shell: welcome (no session) or 3-column repo view.
  // T03 wires open/init/close/recent/trust to real IPC; T05/T06 wire history
  // and search; T07 wires worktree status with focus/event refresh.
  import { onMount, onDestroy, untrack } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import BitbucketAuthModal from "../lib/components/BitbucketAuthModal.svelte";
  import DiscardConfirmModal from "../lib/components/DiscardConfirmModal.svelte";
  import GitActions from "../lib/components/GitActions.svelte";
  import GitToolbar from "../lib/components/GitToolbar.svelte";
  import HelpModal from "../lib/components/HelpModal.svelte";
  import MergeModal from "../lib/components/MergeModal.svelte";
  import SettingsModal from "../lib/components/SettingsModal.svelte";
  import StashModal from "../lib/components/StashModal.svelte";
  import DiffPane from "../lib/components/DiffPane.svelte";
  import { DiffController, emptyDiffState } from "../lib/diff/controller";
  import HistoryPane from "../lib/components/HistoryPane.svelte";
  import BranchModal from "../lib/components/BranchModal.svelte";
  import CommitActionModal from "../lib/components/CommitActionModal.svelte";
  import StashSwitchModal from "../lib/components/StashSwitchModal.svelte";
  import { COMMIT_ACTION_FORMS } from "../lib/history/commit-action-forms";
  import type { CommitActionId } from "../lib/history/commit-menu";
  import type { BranchFormOverride } from "../lib/history/commit-action-forms";
  import type { BranchMenuAction } from "../lib/refs/branch-menu";
  import Inspector from "../lib/components/Inspector.svelte";
  import Sidebar from "../lib/components/Sidebar.svelte";
  import Splitter from "../lib/components/Splitter.svelte";
  import StatusBar from "../lib/components/StatusBar.svelte";
  import TopBar from "../lib/components/TopBar.svelte";
  import { isNative, newRequestId } from "../lib/ipc/client";
  import type { mockAdapter as MockAdapter } from "../lib/ipc/mock";
  import type { WorkspaceState } from "../lib/repositories/tabs";
  import { autoStashMessage, needsStashOffer } from "../lib/repositories/tabs";
  import { asSyncKind, syncFailureMessage, type SyncKind } from "../lib/sync/errors";
  import { BITBUCKET_TOKEN_URL, isBitbucketCloudHttps } from "../lib/sync/bitbucket";
  import { realAdapter } from "../lib/ipc/real";
  import type {
    AppError,
    ChangedFile,
    CommitDetails,
    CommitRow,
    ConflictFile,
    ConflictPreview,
    HistoryScope,
    OperationLogEntry,
    OperationRecord,
    PreflightData,
    RebasePlanEntry,
    RefItem,
    RemoteStatus,
    RepoInvalidatedEvent,
    RepoSnapshot,
    SettingsV1,
    StashEntry
  } from "../lib/ipc/types";
  import { collectDiscardAllTargets, discardAllSummary } from "../lib/status/discard-all";
  import { summarizeWorkingChanges } from "../lib/status/partition";
  import { suggestedTrackName } from "../lib/refs/filter";
  import { SearchController } from "../lib/search/controller";
  import { clampWidth, initialShell, SHELL_LIMITS, type InspectorState } from "../lib/state/shell";
  import { layoutGraph } from "../lib/graph/layout";
  import { headLabel } from "../mocks/demoSession";

  const HISTORY_LIMIT = 200;
  const SEARCH_DEBOUNCE_MS = 250;
  const ALL_REFS_SCOPE: HistoryScope = { type: "allRefs" };

  let { initialSession, active, mockAdapter, onOpenRepository, onInitRepository, onCloseRepository, onWorkspaceChange }: {
    initialSession: RepoSnapshot; active: boolean; mockAdapter: typeof MockAdapter;
    onOpenRepository: () => void; onInitRepository: () => void; onCloseRepository: () => void;
    onWorkspaceChange: (state: WorkspaceState) => void;
  } = $props();
  const demo = !isNative();
  const shell = $state(initialShell());
  let section = $state("working");
  let searchQuery = $state("");
  let preflight: PreflightData | null = $state(null);
  let session: RepoSnapshot | null = $state(untrack(() => initialSession));
  let busy = $state(false);
  let sessionError: AppError | null = $state(null);
  let topBar: { focusSearch: () => void } | undefined = $state(undefined);

  // History integration (T05): rows accumulate across pages; lanes carry on.
  let historyRows: CommitRow[] = $state([]);
  let historyCursor: string | null = $state(null);
  let historyHasMore = $state(false);
  let historyTruncated = $state(false);
  let historyLoading = $state(false);
  let historyLoadingMore = $state(false);
  let historyError: AppError | null = $state(null);
  let refLabels: Map<string, string> = $state(new Map());
  let commitDetails: CommitDetails | null = $state(null);
  let detailsLoading = $state(false);
  let detailsError: AppError | null = $state(null);

  // Navigation scope + search (T06). The controller discards out-of-order
  // search responses; switching repo/scope/clear invalidates in-flight work.
  let scope: HistoryScope = $state({ ...ALL_REFS_SCOPE });
  let scopeValue = $state("all");
  let refs: RefItem[] = $state([]);
  const searchController = new SearchController();
  let searchTimer: ReturnType<typeof setTimeout> | undefined = undefined;
  let lastScheduledQuery = "";
  let searchRows: CommitRow[] = $state([]);
  let searchCursor: string | null = $state(null);
  let searchHasMore = $state(false);
  let searchIncomplete = $state(false);
  let searchLoading = $state(false);
  let searchLoadingMore = $state(false);
  let searchError: AppError | null = $state(null);

  // Worktree status (T07). Null rows = never loaded; errors keep stale rows
  // when a previous listing exists, so a failed refresh never blanks the UI.
  let statusFiles: ChangedFile[] | null = $state(null);
  // Main-panel bar: one-line unstaged summary, hidden when the tree is clean.
  const workBar = $derived(summarizeWorkingChanges(statusFiles));
  let statusLoading = $state(false);
  let statusError: AppError | null = $state(null);
  let unlistenInvalidated: (() => void) | null = null;

  // The central diff panel owns its selection and invalidates late responses.
  let diff = $state(emptyDiffState());
  let diffOpener: HTMLElement | null = null;
  const diffController = new DiffController(
    (repoId, target) => statusAdapter().diffRead(repoId, target),
    (state) => {
      diff = state;
      shell.selectedFile = state.selection?.path ?? null;
    }
  );
  let detailsRequest = 0;

  // Index/worktree mutations (T09). Files act directly; destructive
  // discard uses a state-bound confirmation token.
  let indexBusy = $state(false);
  let indexError: AppError | null = $state(null);
  let discardConfirm: { kind: "file" | "hunk" | "all"; pathId: string; hunkId: string | null; summary: string; token: string; allPaths: string[] } | null = $state(null);
  let discardBusy = $state(false);
  let discardError: string | null = $state(null);

  const selectedWorktreeFile = $derived.by(() => {
    const target = diff.selection?.target;
    if (target?.kind !== "worktree") return null;
    return statusFiles?.find((file) => file.pathId === target.pathId) ?? null;
  });
  const hunkMutationHint = $derived.by(() => {
    if (diff.selection?.target.kind !== "worktree") return null;
    if (selectedWorktreeFile?.worktreeStatus === "?") return "Partial actions are unavailable for untracked files. Use Stage file or Discard file.";
    if (diff.doc?.truncated) return "Partial actions are unavailable while this diff is truncated.";
    if (diff.doc?.kind !== "text") return "Partial actions require a text diff.";
    return null;
  });
  const canMutateHunks = $derived(diff.selection?.target.kind === "worktree" && hunkMutationHint === null && !indexBusy);

  // Commit editor (T10). Subject lives in shell (survives tab switches);
  // body is session-local. Both survive failures, cleared only on success.
  let commitBody = $state("");
  let commitBusy = $state(false);
  let commitError: AppError | null = $state(null);
  let identityLabel: string | null = $state(null);

  // Branch dialog (T10).
  let showBranches = $state(false);
  let branchBusy = $state(false);
  let branchError: AppError | string | null = $state(null);
  let newBranchName = $state("");
  let switchAfterCreate = $state(true);
  let branchStartOid: string | null = $state(null);
  let trackName = $state("");
  let deleteConfirm: { refId: string; summary: string; token: string } | null = $state(null);

  // Stash-and-switch offer when the worktree is dirty.
  type StashSwitchTarget =
    | { kind: "branch"; refId: string; label: string; trackName: string | null }
    | { kind: "checkout"; oid: string; label: string };
  let stashSwitch: StashSwitchTarget | null = $state(null);
  let stashSwitchIncludeUntracked = $state(true);
  let stashSwitchBusy = $state(false);
  let stashSwitchError: AppError | null = $state(null);
  let stashSwitchDone: string | null = $state(null);

  // Branch-row forms from the sidebar menu (rename / upstream / move / push).
  let branchForm: { kind: "rename" | "upstream" | "move" | "push"; ref: RefItem; targetOid: string | null } | null = $state(null);

  // Commit history actions (T18): one modal per action except create-branch.
  let historyAction: { action: CommitActionId; row: CommitRow } | null = $state(null);
  let historyActionBusy = $state(false);
  let historyActionError: AppError | null = $state(null);
  let historyConfirmSummary: string | null = $state(null);
  let historyConfirmToken: string | null = $state(null);

  // Remote sync (T11). remoteStatus is read-only and never touches the
  // network; fetch/pull/push run as cancellable background jobs tracked
  // through operation events with an operationGet poll fallback.
  let remoteStatus: RemoteStatus | null = $state(null);
  let syncJob: OperationRecord | null = $state(null);
  let syncError: string | null = $state(null);
  let syncErrorCode: string | null = $state(null);
  let syncRetryKind: SyncKind | null = $state(null);
  let syncLogOpen = $state(false);
  let syncLogEntries: OperationLogEntry[] = $state([]);
  let syncLogLoading = $state(false);
  let syncLogError: string | null = $state(null);
  let showBitbucketAuth = $state(false);
  let bitbucketToken = $state("");
  let bitbucketBusy = $state(false);
  let bitbucketError: AppError | null = $state(null);
  let bitbucketRetryKind: SyncKind | null = $state(null);
  let unlistenOperation: (() => void) | null = null;
  let syncPoll: ReturnType<typeof setInterval> | undefined = undefined;

  // One layout snapshot provides consistent widths and lanes across pages.
  const graph = $derived(layoutGraph(historyRows, new Map()));
  const laidRows = $derived(graph.rows);
  const laneCount = $derived(graph.laneCount);

  function historyAdapter() {
    return demo ? mockAdapter : realAdapter;
  }

  function scopeLabel(value: string): string {
    if (value === "all") return "all refs";
    if (value === "head") return "HEAD";
    return refs.find((r) => r.refId === value.slice(4))?.label ?? value;
  }

  const scopeOptions = $derived.by(() => {
    const options = [{ value: "all", label: "All refs" }];
    const head = session?.head;
    if (head) {
      const headScope =
        head.kind === "branch"
          ? `HEAD (${head.name})`
          : head.kind === "detached"
            ? `HEAD (detached ${head.oid.slice(0, 8)})`
            : `HEAD (unborn ${head.name})`;
      options.push({ value: "head", label: headScope });
    }
    for (const ref of refs) options.push({ value: `ref:${ref.refId}`, label: ref.label });
    return options;
  });

  const activeRefId = $derived(scope.type === "ref" ? (scope.refId ?? null) : null);
  const searchActive = $derived(searchQuery.trim() !== "");
  const emptyHint = $derived.by(() => {
    const head = session?.head;
    if (head?.kind === "unborn")
      return `No commits yet — HEAD is unborn on “${head.name}”. The first commit will appear here.`;
    if (scope.type !== "allRefs") return `No commits reachable from ${scopeLabel(scopeValue)}.`;
    return "No commits yet. Working changes are available in the inspector.";
  });

  async function loadHistory(first: boolean): Promise<void> {
    if (!session) return;
    const current = session;
    if (first) {
      historyLoading = true;
      historyRows = [];
      historyCursor = null;
      historyTruncated = false;
      refLabels = new Map();
    } else {
      historyLoadingMore = true;
    }
    historyError = null;
    try {
      const adapter = historyAdapter();
      const activeScope = scope;
      const [refList, page] = await Promise.all([
        first ? adapter.repoRefs(current.repoId) : Promise.resolve(null),
        adapter.historyPage(current.repoId, activeScope, historyCursor, HISTORY_LIMIT)
      ]);
      if (refList) {
        const labels = new Map<string, string>();
        for (const ref of refList) labels.set(ref.refId, ref.label);
        refLabels = labels;
        refs = [...refList];
      }
      historyRows = [...historyRows, ...page.rows];
      historyCursor = page.nextCursor;
      historyHasMore = page.nextCursor !== null;
      // The backend flags truncation only on the final cached page.
      if (page.nextCursor === null) historyTruncated = page.truncated;
    } catch (e) {
      historyError = e as AppError;
    } finally {
      historyLoading = false;
      historyLoadingMore = false;
    }
  }

  async function loadDetails(oid: string, parentIndex: number | null): Promise<void> {
    if (!session) return;
    const repoId = session.repoId;
    const request = ++detailsRequest;
    const current = () => request === detailsRequest && session?.repoId === repoId && shell.selectedCommitOid === oid;
    detailsLoading = true;
    detailsError = null;
    commitDetails = null;
    try {
      const details = await historyAdapter().commitDetails(repoId, oid, parentIndex);
      if (current()) commitDetails = details;
    } catch (e) {
      if (current()) detailsError = e as AppError;
    } finally {
      if (current()) detailsLoading = false;
    }
  }

  async function runSearch(repoId: string, query: string, first: boolean): Promise<void> {
    const token = searchController.begin();
    if (first) {
      searchLoading = true;
      searchRows = [];
      searchCursor = null;
    } else {
      searchLoadingMore = true;
    }
    searchError = null;
    try {
      const page = await historyAdapter().historySearch(repoId, scope, query, searchCursor, HISTORY_LIMIT);
      // Stale response, or the repo changed underneath: never commit.
      if (!searchController.isCurrent(token)) return;
      if (session === null || session.repoId !== repoId) return;
      searchRows = first ? [...page.rows] : [...searchRows, ...page.rows];
      searchCursor = page.nextCursor;
      searchHasMore = page.nextCursor !== null;
      searchIncomplete = page.incomplete;
    } catch (e) {
      if (!searchController.isCurrent(token)) return;
      if (session === null || session.repoId !== repoId) return;
      searchError = e as AppError;
    } finally {
      if (searchController.isCurrent(token)) {
        searchLoading = false;
        searchLoadingMore = false;
      }
    }
  }

  function resetSearch(): void {
    clearTimeout(searchTimer);
    searchController.invalidate();
    searchRows = [];
    searchCursor = null;
    searchHasMore = false;
    searchIncomplete = false;
    searchError = null;
    searchLoading = false;
    searchLoadingMore = false;
  }


  function resetStatus(): void {
    statusFiles = null;
    statusLoading = false;
    statusError = null;
  }

  function statusAdapter() {
    return demo ? mockAdapter : realAdapter;
  }

  /** Fresh worktree listing. Commits only when the repo is still current. */
  async function loadStatus(): Promise<void> {
    if (!session) {
      resetStatus();
      return;
    }
    const current = session;
    statusLoading = true;
    statusError = null;
    try {
      const data = await statusAdapter().repoStatus(current.repoId);
      if (session === null || session.repoId !== current.repoId) return;
      statusFiles = [...data.files];
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      // Keep previous rows (if any); a failed refresh never blanks the list.
      statusError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId) statusLoading = false;
    }
  }

  /** Quiet re-read after focus or an invalidation event. */
  async function refreshStatusQuiet(): Promise<void> {
    if (!session || statusLoading) return;
    if (diff.selection && diff.selection.target.kind !== "commit") clearDiff();
    await loadStatus();
  }

  async function refreshActivatedRepository(): Promise<void> {
    const current = session;
    if (!current || busy || indexBusy || commitBusy || branchBusy || syncJobActive || mergeBusy || conflictBusy || stashSaveBusy || stashBusyEntry) return;
    try {
      const fresh = demo ? await mockAdapter.repoSnapshot() : await realAdapter.repoSnapshot(current.repoId, false);
      if (disposed) return;
      const headChanged = JSON.stringify(current.head) !== JSON.stringify(fresh.head);
      session = fresh;
      await refreshStatusQuiet();
      await loadRemoteStatus();
      if (headChanged) await loadHistory(true);
    } catch (e) { if (!disposed) sessionError = e as AppError; }
  }

  function unsubscribeInvalidated(): void {
    unlistenInvalidated?.();
    unlistenInvalidated = null;
    unlistenOperation?.();
    unlistenOperation = null;
  }

  function stopSyncPoll(): void {
    if (syncPoll !== undefined) {
      clearInterval(syncPoll);
      syncPoll = undefined;
    }
  }


  function syncAdapter() {
    return demo ? mockAdapter : realAdapter;
  }

  /** Read-only remote context: effective upstream, ahead/behind, last fetch. */
  async function loadRemoteStatus(): Promise<void> {
    if (!session) {
      remoteStatus = null;
      return;
    }
    const current = session;
    try {
      const status = await syncAdapter().remoteStatus(current.repoId);
      if (session === null || session.repoId !== current.repoId) return;
      remoteStatus = status;
    } catch {
      if (session === null || session.repoId !== current.repoId) return;
      remoteStatus = null;
    }
  }

  const remoteLabel = $derived.by(() => {
    const upstream = remoteStatus?.upstreamRef ?? remoteStatus?.remoteName ?? null;
    if (!upstream) return null;
    const ahead = remoteStatus?.ahead ?? 0;
    const behind = remoteStatus?.behind ?? 0;
    return ahead !== 0 || behind !== 0 ? `${upstream} · ↑${ahead} ↓${behind}` : upstream;
  });

  const remoteTitle = $derived.by(() => {
    if (!remoteStatus) return "No remote information for this repository";
    const url = remoteStatus.url ?? "unknown URL";
    const fetched = remoteStatus.lastFetchAt ?? "never fetched";
    return `${url} · last fetch: ${fetched}`;
  });

  const bitbucketAvailable = $derived.by(() => isBitbucketCloudHttps(remoteStatus?.url ?? null));

  const syncDisabled = $derived.by(() => {
    const current = session;
    if (!current) return true;
    return current.trust === "readOnly";
  });
  const syncDisabledReason = $derived(
    !session
      ? "Open a repository first"
      : "Trust this repository to contact remotes (network operations need trust)"
  );

  const syncJobActive = $derived.by(() => {
    const job = syncJob;
    if (!job) return false;
    return job.state === "queued" || job.state === "running" || job.state === "cancelling";
  });

  const syncJobLabel = $derived.by(() => {
    if (!syncJob) return "";
    const verb = syncJob.kind === "fetch" ? "Fetching" : syncJob.kind === "pull" ? "Pulling" : syncJob.kind === "push" ? "Pushing" : "Working";
    return syncJob.stage && syncJob.stage !== "" && syncJob.stage !== "contacting remote"
      ? `${verb} (${syncJob.stage})`
      : verb;
  });

  function syncTerminal(state: string): boolean {
    return state === "succeeded" || state === "failed" || state === "cancelled";
  }

  /** Refresh everything a finished job may have changed. */
  async function afterSyncJob(record: OperationRecord): Promise<void> {
    const repoId = record.repoId;
    if (!session || (repoId !== null && session.repoId !== repoId)) return;
    if (record.state === "succeeded") {
      syncError = null;
      syncErrorCode = null;
      syncRetryKind = null;
      try {
        if (!demo) session = await realAdapter.repoSnapshot(session.repoId, true);
      } catch {
        // Keep the last snapshot; the listings below still refresh.
      }
      await loadHistory(true);
      await loadStatus();
      await loadRemoteStatus();
    } else if (record.state === "failed") {
      syncErrorCode = record.error?.code ?? record.errorCode ?? "GIT_ERROR";
      syncRetryKind = asSyncKind(record.kind);
      syncError = syncFailureMessage(record.kind, syncErrorCode, record.error, remoteStatus?.url ?? null);
      await loadRemoteStatus();
    } else if (record.state === "cancelled") {
      syncError = `${record.kind} was cancelled.`;
      syncErrorCode = null;
      syncRetryKind = null;
    }
    if (syncLogOpen) await loadOperationLog();
  }

  /** Poll fallback: events are best-effort, the record is authoritative. */
  async function trackSyncJob(repoId: string, operationId: string): Promise<void> {
    stopSyncPoll();
    try {
      syncJob = await syncAdapter().operationGet(operationId);
    } catch (e) {
      syncError = (e as AppError).message ?? "Could not read the operation.";
      return;
    }
    if (session === null || session.repoId !== repoId) return;
    if (syncTerminal(syncJob.state)) {
      await afterSyncJob(syncJob);
      return;
    }
    syncPoll = setInterval(() => {
      void (async () => {
        try {
          const record = await syncAdapter().operationGet(operationId);
          if (session === null || session.repoId !== repoId) {
            stopSyncPoll();
            return;
          }
          syncJob = record;
          if (syncTerminal(record.state)) {
            stopSyncPoll();
            await afterSyncJob(record);
          }
        } catch {
          // Keep the last known state; the next tick retries.
        }
      })();
    }, 600);
  }

  async function startSyncJob(kind: "fetch" | "pull" | "push"): Promise<void> {
    if (!session || syncJobActive) return;
    const current = session;
    syncError = null;
    syncErrorCode = null;
    syncRetryKind = null;
    try {
      const adapter = syncAdapter();
      const started =
        kind === "fetch"
          ? await adapter.remoteFetch(current.repoId, current.version, null)
          : kind === "pull"
            ? await adapter.remotePull(current.repoId, current.version)
            : await adapter.remotePush(current.repoId, current.version, null, true);
      await trackSyncJob(current.repoId, started.operationId);
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      const error = e as AppError;
      syncErrorCode = error.code ?? "GIT_ERROR";
      syncRetryKind = kind;
      syncError = syncFailureMessage(kind, syncErrorCode, error, remoteStatus?.url ?? null);
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
    }
  }

  async function cancelSyncJob(): Promise<void> {
    if (!syncJob || !syncJobActive) return;
    try {
      await syncAdapter().operationCancel(syncJob.operationId);
      syncJob = { ...syncJob, state: "cancelling" };
    } catch (e) {
      syncError = (e as AppError).message ?? "Could not cancel the operation.";
    }
  }

  async function loadOperationLog(): Promise<void> {
    if (!session) return;
    const current = session;
    syncLogLoading = true;
    syncLogError = null;
    try {
      const page = await syncAdapter().operationLog(current.repoId, 0);
      if (session === null || session.repoId !== current.repoId) return;
      syncLogEntries = [...page.entries];
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      syncLogError = (e as AppError).message ?? "Could not load the operation log.";
    } finally {
      if (session?.repoId === current.repoId) syncLogLoading = false;
    }
  }

  function toggleSyncLog(): void {
    syncLogOpen = !syncLogOpen;
    if (syncLogOpen) void loadOperationLog();
  }

  function openBitbucketAuth(): void {
    if (!bitbucketAvailable || syncJobActive) return;
    bitbucketToken = "";
    bitbucketError = null;
    bitbucketRetryKind = syncRetryKind;
    showBitbucketAuth = true;
  }

  function closeBitbucketAuth(): void {
    if (bitbucketBusy) return;
    bitbucketToken = "";
    bitbucketError = null;
    bitbucketRetryKind = null;
    showBitbucketAuth = false;
  }

  async function openBitbucketTokenPage(): Promise<void> {
    try {
      if (demo) window.open(BITBUCKET_TOKEN_URL, "_blank", "noopener,noreferrer");
      else await openUrl(BITBUCKET_TOKEN_URL);
    } catch {
      bitbucketError = {
        code: "IO_ERROR",
        message: "Could not open the Atlassian token page. Open it in your browser manually.",
        recovery: "retryRead",
        retryable: true
      };
    }
  }

  async function connectBitbucket(): Promise<void> {
    if (!session || !bitbucketAvailable || bitbucketBusy || bitbucketToken.trim() === "") return;
    const current = session;
    const token = bitbucketToken;
    const retryPush = bitbucketRetryKind === "push";
    bitbucketBusy = true;
    bitbucketError = null;
    try {
      await syncAdapter().bitbucketConnect(
        current.repoId,
        current.version,
        remoteStatus?.remoteName ?? null,
        token
      );
      if (session === null || session.repoId !== current.repoId) return;
      bitbucketToken = "";
      bitbucketRetryKind = null;
      showBitbucketAuth = false;
      syncError = null;
      syncErrorCode = null;
      syncRetryKind = null;
      if (retryPush) await startSyncJob("push");
    } catch (error) {
      if (session === null || session.repoId !== current.repoId) return;
      bitbucketError = error as AppError;
    } finally {
      if (session?.repoId === current.repoId) bitbucketBusy = false;
    }
  }

  // Stash workflows (T12). Entries identify by OID; a reorder underneath
  // fails in the backend with STALE_STATE instead of touching the wrong one.
  let showStash = $state(false);
  let stashEntries: StashEntry[] = $state([]);
  let stashLoading = $state(false);
  let stashError: AppError | null = $state(null);
  let stashNotice: string | null = $state(null);
  let stashMessage = $state("");
  let stashIncludeUntracked = $state(false);
  let stashSaveBusy = $state(false);
  let stashBusyEntry: string | null = $state(null);


  function stashAdapter() {
    return demo ? mockAdapter : realAdapter;
  }

  async function loadStashList(): Promise<void> {
    if (!session) return;
    const current = session;
    stashLoading = true;
    stashError = null;
    try {
      const entries = await stashAdapter().stashList(current.repoId);
      if (session === null || session.repoId !== current.repoId) return;
      stashEntries = [...entries];
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      stashError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId) stashLoading = false;
    }
  }

  function openStash(): void {
    if (!session) return;
    stashError = null;
    stashNotice = null;
    showStash = true;
    void loadStashList();
  }

  function closeStash(): void {
    showStash = false;
    stashBusyEntry = null;
  }

  // Preferences + local UI persistence (T14). Settings v1 lives in the
  // native store (mock in demo); drafts and panel widths live in
  // localStorage keyed per repository / globally.
  let showSettings = $state(false);
  let showHelp = $state(false);
  let settings: SettingsV1 | null = $state(null);
  let settingsDraftScale = $state(1);
  let settingsBusy = $state(false);
  let settingsError: AppError | null = $state(null);

  const DRAFTS_KEY = "gitdock.drafts.v2";
  const WIDTHS_KEY = "gitdock.widths.v1";

  function readJson(key: string): Record<string, unknown> {
    try {
      const raw = localStorage.getItem(key);
      if (!raw) return {};
      const parsed: unknown = JSON.parse(raw);
      return typeof parsed === "object" && parsed !== null
        ? (parsed as Record<string, unknown>)
        : {};
    } catch {
      return {};
    }
  }

  function applyFontScale(scale: number): void {
    const px = (base: number): string => `${Math.round(base * scale * 10) / 10}px`;
    const root = document.documentElement;
    root.style.setProperty("--gd-font-size", px(13));
    root.style.setProperty("--gd-font-size-small", px(12));
    root.style.setProperty("--gd-font-size-title", px(15));
  }

  async function loadSettings(): Promise<void> {
    settingsError = null;
    try {
      const next = demo ? await mockAdapter.settingsGet() : await realAdapter.settingsGet();
      settings = next;
      settingsDraftScale = next.fontScale;
      applyFontScale(next.fontScale);
    } catch (e) {
      settingsError = e as AppError;
    }
  }

  async function saveSettings(): Promise<void> {
    if (!settings || settingsBusy) return;
    const current = settings;
    settingsBusy = true;
    settingsError = null;
    try {
      const next = demo
        ? await mockAdapter.settingsUpdate(current.version, settingsDraftScale)
        : await realAdapter.settingsUpdate(current.version, settingsDraftScale);
      settings = next;
      settingsDraftScale = next.fontScale;
      applyFontScale(next.fontScale);
    } catch (e) {
      settingsError = e as AppError;
      // A concurrent writer wins: reload instead of sitting on stale state.
      await loadSettings();
    } finally {
      settingsBusy = false;
    }
  }

  function persistDraft(): void {
    if (!session) return;
    try {
      const drafts = readJson(DRAFTS_KEY);
      drafts[session.workspaceKey] = { subject: shell.commitMessage, body: commitBody };
      localStorage.setItem(DRAFTS_KEY, JSON.stringify(drafts));
    } catch {
      // Drafts are best-effort; the commit boxes always work without them.
    }
  }

  function restoreDraft(workspaceKey: string): void {
    const entry = (readJson(DRAFTS_KEY)[workspaceKey] ?? readJson("gitdock.drafts.v1")[session?.repoId ?? ""]) as
      | { subject?: unknown; body?: unknown }
      | undefined;
    shell.commitMessage = typeof entry?.subject === "string" ? entry.subject : "";
    commitBody = typeof entry?.body === "string" ? entry.body : "";
  }

  function persistWidths(): void {
    try {
      localStorage.setItem(
        WIDTHS_KEY,
        JSON.stringify({ sidebar: shell.sidebarWidth, inspector: shell.inspectorWidth })
      );
    } catch {
      // Widths are best-effort.
    }
  }

  function restoreWidths(): void {
    const stored = readJson(WIDTHS_KEY);
    if (typeof stored.sidebar === "number") {
      shell.sidebarWidth = clampWidth(stored.sidebar, SHELL_LIMITS.sidebarMin, SHELL_LIMITS.sidebarMax);
    }
    if (typeof stored.inspector === "number") {
      shell.inspectorWidth = clampWidth(
        stored.inspector,
        SHELL_LIMITS.inspectorMin,
        SHELL_LIMITS.inspectorMax
      );
    }
  }

  // Merge + conflict inspector (T13). The conflict tab reads live
  // unmerged index state; path ids always come from the last listing.
  let showMerge = $state(false);
  let mergeSource = $state("");
  let mergeBusy = $state(false);
  let mergeError: AppError | null = $state(null);
  let conflictFiles: ConflictFile[] | null = $state(null);
  let conflictLoading = $state(false);
  let conflictError: AppError | null = $state(null);
  let conflictSelected: string | null = $state(null);
  let conflictPreview: ConflictPreview | null = $state(null);
  let conflictPreviewLoading = $state(false);
  let conflictPreviewError: AppError | null = $state(null);
  let conflictBusy: string | null = $state(null);
  let conflictActionError: AppError | null = $state(null);
  let conflictNotice: string | null = $state(null);
  let mergeSubject = $state("");
  let reviewedStaged = $state(false);
  let canComplete = $state(false);
  let canAbort = $state(false);
  let abortReason: string | null = $state(null);
  let abortConfirm: string | null = $state(null);
  let acceptConfirm: { side: string; summary: string; token: string } | null = $state(null);


  function mergeAdapter() {
    return demo ? mockAdapter : realAdapter;
  }

  const mergeBanner = $derived.by(() => {
    const current = session;
    if (!current) return null;
    if (current.state === "merging") {
      const origin = current.mergeOrigin === "app" ? " (started here)" : current.mergeOrigin === "external" ? " (started outside)" : "";
      return `Merge in progress${origin}: resolve each file, then complete.`;
    }
    if (current.state === "stashConflict") {
      return "A stashed change left conflicts: resolve them in Working changes, then commit normally.";
    }
    return null;
  });

  const mergeSources = $derived.by(() => {
    const head = session?.head;
    const current = head?.kind === "branch" ? head.name : null;
    // Local branches except current, then remote tracking branches: the
    // engine accepts any live ref as a merge source.
    return [
      ...refs.filter((r) => r.kind === "local" && r.label !== current),
      ...refs.filter((r) => r.kind === "remote")
    ];
  });

  const mergeTargetLabel = $derived.by(() => {
    const head = session?.head;
    if (!head) return "HEAD";
    if (head.kind === "branch") return head.name;
    if (head.kind === "detached") return `detached ${head.oid.slice(0, 8)}`;
    return `unborn ${head.name}`;
  });

  const mergeTargetOid = $derived.by(() => {
    const head = session?.head;
    return head && head.kind !== "unborn" ? head.oid : null;
  });

  function openMerge(): void {
    if (!session) return;
    mergeError = null;
    const first = mergeSources[0]?.refId ?? "";
    if (!mergeSource || !mergeSources.some((r) => r.refId === mergeSource)) mergeSource = first;
    showMerge = true;
  }

  async function startMerge(): Promise<void> {
    if (!session || mergeBusy || !mergeSource || !mergeTargetOid) return;
    const current = session;
    mergeBusy = true;
    mergeError = null;
    conflictNotice = null;
    try {
      const result = await mergeAdapter().mergeStart(
        current.repoId,
        current.version,
        mergeSource,
        mergeTargetOid
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      showMerge = false;
      changeInspector("conflict");
      conflictNotice = result.alreadyUpToDate
        ? "Already up to date — nothing to merge."
        : result.conflicted
          ? "Conflicts need resolution below."
          : "Merged cleanly — review the staged result, then complete.";
      await loadStatus();
      await loadConflicts();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      mergeError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
    } finally {
      if (session?.repoId === current.repoId) mergeBusy = false;
    }
  }

  async function loadConflicts(selectFirst: boolean = true): Promise<void> {
    if (!session) return;
    const current = session;
    conflictLoading = true;
    conflictError = null;
    try {
      const list = await mergeAdapter().conflictList(current.repoId);
      if (session === null || session.repoId !== current.repoId) return;
      conflictFiles = [...list.files];
      canComplete = list.canComplete;
      canAbort = list.canAbort;
      abortReason = list.abortReason;
      reviewedStaged = true;
      if (selectFirst) {
        conflictSelected = list.files[0]?.pathId ?? null;
        conflictPreview = null;
        conflictPreviewError = null;
        if (conflictSelected) await loadConflictPreview(conflictSelected);
      } else if (conflictSelected && !list.files.some((f) => f.pathId === conflictSelected)) {
        conflictSelected = list.files[0]?.pathId ?? null;
        conflictPreview = null;
        if (conflictSelected) await loadConflictPreview(conflictSelected);
      }
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId) conflictLoading = false;
    }
  }

  async function loadConflictPreview(pathId: string): Promise<void> {
    if (!session) return;
    const current = session;
    conflictPreviewLoading = true;
    conflictPreviewError = null;
    try {
      const preview = await mergeAdapter().conflictPreview(current.repoId, pathId);
      if (session === null || session.repoId !== current.repoId) return;
      if (conflictSelected !== pathId) return;
      conflictPreview = preview;
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      if (conflictSelected !== pathId) return;
      conflictPreview = null;
      conflictPreviewError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId) conflictPreviewLoading = false;
    }
  }

  function selectConflict(pathId: string): void {
    acceptConfirm = null;
    conflictSelected = pathId;
    conflictPreview = null;
    conflictPreviewError = null;
    void loadConflictPreview(pathId);
  }

  async function askAccept(side: string): Promise<void> {
    if (!session || !conflictSelected || conflictBusy !== null) return;
    const current = session;
    const pathId = conflictSelected;
    conflictActionError = null;
    conflictBusy = `accept-${side}`;
    try {
      const details = await mergeAdapter().confirmationPrepare(
        current.repoId,
        current.version,
        "conflict_accept",
        [pathId, side]
      );
      if (session === null || session.repoId !== current.repoId) return;
      acceptConfirm = { side, summary: details.summary, token: details.confirmationToken };
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId && conflictBusy === `accept-${side}`) {
        conflictBusy = null;
      }
    }
  }

  async function confirmAccept(): Promise<void> {
    if (!session || !conflictSelected || !conflictPreview || !acceptConfirm) return;
    const current = session;
    const pathId = conflictSelected;
    const { side, token } = acceptConfirm;
    conflictActionError = null;
    conflictNotice = null;
    conflictBusy = `accept-${side}`;
    try {
      const result = await mergeAdapter().conflictAccept(
        current.repoId,
        current.version,
        pathId,
        side,
        conflictPreview.workingFingerprint,
        token
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      acceptConfirm = null;
      conflictNotice = `Applied the ${side} version (unstaged). Mark it resolved when the file looks right.`;
      await loadStatus();
      await loadConflicts(false);
      if (conflictSelected === pathId) await loadConflictPreview(pathId);
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
      acceptConfirm = null;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
      await loadStatus();
      await loadConflicts(false);
    } finally {
      if (session?.repoId === current.repoId) conflictBusy = null;
    }
  }

  async function markResolved(resolution: "workingFile" | "deletion"): Promise<void> {
    if (!session || !conflictSelected || !conflictPreview || conflictBusy !== null) return;
    const current = session;
    const pathId = conflictSelected;
    conflictActionError = null;
    conflictNotice = null;
    conflictBusy = "resolve";
    try {
      session = await mergeAdapter().conflictMarkResolved(
        current.repoId,
        current.version,
        pathId,
        conflictPreview.workingFingerprint,
        resolution
      );
      if (session === null || session.repoId !== current.repoId) return;
      conflictNotice =
        resolution === "deletion"
          ? "Staged the deletion. Complete the merge when every file is resolved."
          : "Staged as resolved. Complete the merge when every file is resolved.";
      await loadStatus();
      await loadConflicts();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
      await loadStatus();
      await loadConflicts(false);
    } finally {
      if (session?.repoId === current.repoId) conflictBusy = null;
    }
  }

  async function completeMerge(): Promise<void> {
    if (!session || conflictBusy !== null || mergeSubject.trim() === "" || !reviewedStaged) return;
    const current = session;
    const head = current.head;
    if (head.kind === "unborn") return;
    conflictActionError = null;
    conflictNotice = null;
    conflictBusy = "complete";
    try {
      const result = await mergeAdapter().mergeComplete(
        current.repoId,
        current.version,
        mergeSubject.trim(),
        "",
        reviewedStaged,
        head.oid
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      mergeSubject = "";
      reviewedStaged = false;
      changeInspector("working");
      conflictNotice = null;
      await loadHistory(true);
      await loadStatus();
      await loadRemoteStatus();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
    } finally {
      if (session?.repoId === current.repoId) conflictBusy = null;
    }
  }

  const abortConfirmToken: { current: string | null } = { current: null };

  async function askAbort(): Promise<void> {
    if (!session || conflictBusy !== null) return;
    const current = session;
    conflictActionError = null;
    conflictBusy = "abort";
    try {
      const details = await mergeAdapter().confirmationPrepare(
        current.repoId,
        current.version,
        "merge_abort",
        ["merge"]
      );
      if (session === null || session.repoId !== current.repoId) return;
      abortConfirm = details.summary;
      abortConfirmToken.current = details.confirmationToken;
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId && conflictBusy === "abort") conflictBusy = null;
    }
  }

  async function confirmAbort(): Promise<void> {
    if (!session || !abortConfirmToken.current || conflictBusy !== null) return;
    const current = session;
    const token = abortConfirmToken.current;
    conflictActionError = null;
    conflictBusy = "abort";
    try {
      session = await mergeAdapter().mergeAbort(current.repoId, current.version, token);
      if (session === null || session.repoId !== current.repoId) return;
      abortConfirm = null;
      abortConfirmToken.current = null;
      changeInspector("working");
      await loadStatus();
      await loadConflicts(false);
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      conflictActionError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
    } finally {
      if (session?.repoId === current.repoId) conflictBusy = null;
    }
  }

  async function saveStash(): Promise<void> {
    if (!session || stashSaveBusy) return;
    const current = session;
    stashError = null;
    stashNotice = null;
    stashSaveBusy = true;
    try {
      const result = await stashAdapter().stashSave(
        current.repoId,
        current.version,
        stashMessage,
        stashIncludeUntracked
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      stashNotice = result.noChange
        ? "Nothing to stash — the worktree already matches HEAD."
        : `Stashed${result.oid ? ` (${result.oid.slice(0, 8)})` : ""}.`;
      stashMessage = "";
      await loadStatus();
      await loadStashList();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      stashError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
    } finally {
      if (session?.repoId === current.repoId) stashSaveBusy = false;
    }
  }

  async function applyStash(entry: StashEntry, mode: "apply" | "pop"): Promise<void> {
    if (!session || stashBusyEntry !== null) return;
    const current = session;
    stashError = null;
    stashNotice = null;
    stashBusyEntry = `${mode}:${entry.stashId}`;
    try {
      const result = await stashAdapter().stashApply(
        current.repoId,
        current.version,
        entry.stashId,
        entry.oid,
        mode
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      if (result.conflicted) {
        stashNotice =
          `${mode === "pop" ? "Pop" : "Apply"} left conflicts — the entry was kept. ` +
          "Resolve the unmerged files in Working changes, then commit normally.";
      } else if (result.dropError) {
        stashNotice =
          `Applied ${entry.stashId}, but dropping it failed (${result.dropError.code}) — ` +
          "the entry was kept and nothing was applied twice.";
      } else {
        stashNotice =
          mode === "pop"
            ? `Popped ${entry.stashId}.`
            : `Applied ${entry.stashId} (entry kept).`;
      }
      await loadStatus();
      await loadStashList();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      stashError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the error above is what matters.
        }
      }
      await loadStatus();
      await loadStashList();
    } finally {
      if (session?.repoId === current.repoId) stashBusyEntry = null;
    }
  }

  /**
   * Best-effort listener for backend invalidation broadcasts. The query
   * after (re)connect is still `loadStatus` itself, so a missed event is
   * never the only refresh signal (contract §5 listener-race fallback).
   */
  async function subscribeInvalidated(): Promise<void> {
    unsubscribeInvalidated();
    if (demo) return;
    try {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenInvalidated = await listen<RepoInvalidatedEvent>(
        "gitdock://repo-invalidated",
        (event) => {
          const current = session;
          if (!current || event.payload.repoId !== current.repoId) return;
          void refreshStatusQuiet();
        }
      );
      unlistenOperation = await listen<OperationRecord>(
        "gitdock://operation",
        (event) => {
          const record = event.payload;
          const current = session;
          if (!current || !syncJob || record.operationId !== syncJob.operationId) return;
          if (record.repoId !== null && record.repoId !== current.repoId) return;
          syncJob = record;
          if (syncTerminal(record.state)) {
            stopSyncPoll();
            void afterSyncJob(record);
          }
        }
      );
      if (disposed) unsubscribeInvalidated();
    } catch {
      unsubscribeInvalidated();
      unlistenInvalidated = null;
      unlistenOperation = null;
    }
  }

  /** External edits land while the window is unfocused; re-read on return. */
  function handleFocus(): void {
    if (!active || demo || document.hidden) return;
    void refreshStatusQuiet();
  }

  function clearDiff(): void {
    diffController.close();
  }

  function closeDiff(): void {
    clearDiff();
    if (diffOpener?.isConnected) diffOpener.focus();
  }

  function loadWorktreeDiff(pathId: string, kind: "worktree" | "index", path: string): void {
    if (!session) return;
    diffOpener = document.activeElement as HTMLElement | null;
    void diffController.open({ repoId: session.repoId, path, target: { kind, pathId } });
  }

  function loadCommitDiff(pathId: string): void {
    if (!session || !commitDetails) return;
    const file = commitDetails.files.find((f) => f.pathId === pathId);
    if (!file) return;
    diffOpener = document.activeElement as HTMLElement | null;
    void diffController.open({
      repoId: session.repoId,
      path: file.path,
      target: { kind: "commit", oid: commitDetails.oid, parentIndex: commitDetails.parentIndex, pathId }
    });
  }

  /** Stage/unstage exact tokens emitted by the selected inspector group. */
  async function mutateIndex(staged: boolean, pathIds: string[]): Promise<void> {
    if (!session || indexBusy) return;
    const current = session;
    indexError = null;
    const ids = [...new Set(pathIds)];
    if (ids.some(id => !statusFiles?.some(file => file.pathId === id))) {
      indexError = { code: "STALE_STATE", message: "The file list changed. Refresh and select the files again.", recovery: "refresh", retryable: false };
      await loadStatus();
      return;
    }
    if (ids.length === 0) return;
    indexBusy = true;
    try {
      const adapter = statusAdapter();
      session = staged
        ? await adapter.indexStage(current.repoId, current.version, ids)
        : await adapter.indexUnstage(current.repoId, current.version, ids);
      clearDiff();
      await loadStatus();
    } catch (e) {
      indexError = e as AppError;
      // Re-sync: the backend version moved even when the write failed.
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; the status reload still refreshes rows.
        }
      }
      await loadStatus();
    } finally {
      if (session?.repoId === current.repoId) indexBusy = false;
    }
  }

  async function resyncMutationFailure(current: RepoSnapshot): Promise<void> {
    if (!demo) {
      try {
        session = await realAdapter.repoSnapshot(current.repoId, false);
      } catch {
        // Keep the last snapshot; status refresh still reissues path tokens.
      }
    }
    await loadStatus();
  }

  async function refreshOpenWorktreeDiff(displayPath: string): Promise<void> {
    await loadStatus();
    const row = statusFiles?.find((file) => file.displayPath === displayPath && ![" ", "!"].includes(file.worktreeStatus));
    if (!row || !session) {
      clearDiff();
      return;
    }
    await diffController.open({ repoId: session.repoId, path: row.displayPath, target: { kind: "worktree", pathId: row.pathId } });
  }

  async function askDiscardFile(pathId: string): Promise<void> {
    if (!session || indexBusy) return;
    const current = session;
    indexBusy = true;
    indexError = null;
    discardError = null;
    try {
      const details = await statusAdapter().confirmationPrepare(current.repoId, current.version, "discard_file", [pathId]);
      if (session?.repoId !== current.repoId) return;
      discardConfirm = { kind: "file", pathId, hunkId: null, summary: details.summary, token: details.confirmationToken, allPaths: [] };
    } catch (error) {
      indexError = error as AppError;
      await resyncMutationFailure(current);
    } finally {
      if (session?.repoId === current.repoId) indexBusy = false;
    }
  }

  async function askDiscardHunk(hunkId: string): Promise<void> {
    const target = diff.selection?.target;
    if (!session || indexBusy || target?.kind !== "worktree") return;
    const current = session;
    indexBusy = true;
    indexError = null;
    discardError = null;
    try {
      const details = await statusAdapter().confirmationPrepare(current.repoId, current.version, "discard_hunk", [target.pathId, hunkId]);
      if (session?.repoId !== current.repoId) return;
      discardConfirm = { kind: "hunk", pathId: target.pathId, hunkId, summary: details.summary, token: details.confirmationToken, allPaths: [] };
    } catch (error) {
      indexError = error as AppError;
      await resyncMutationFailure(current);
    } finally {
      if (session?.repoId === current.repoId) indexBusy = false;
    }
  }

  async function stageHunk(hunkId: string): Promise<void> {
    const target = diff.selection?.target;
    const displayPath = diff.selection?.path;
    if (!session || indexBusy || target?.kind !== "worktree" || !displayPath) return;
    const current = session;
    indexBusy = true;
    indexError = null;
    try {
      session = await statusAdapter().diffHunkStage(current.repoId, current.version, target.pathId, hunkId);
      await refreshOpenWorktreeDiff(displayPath);
    } catch (error) {
      indexError = error as AppError;
      await resyncMutationFailure(current);
      clearDiff();
    } finally {
      if (session?.repoId === current.repoId) indexBusy = false;
    }
  }

  /** Single confirmation for every unstaged file; execution still uses
   * per-file backend tokens so each discard stays fingerprint-bound. */
  function askDiscardAll(): void {
    if (!session || indexBusy || discardBusy) return;
    const targets = collectDiscardAllTargets(statusFiles);
    if (targets.length === 0) return;
    discardError = null;
    discardConfirm = {
      kind: "all",
      pathId: "",
      hunkId: null,
      summary: discardAllSummary(targets),
      token: "",
      allPaths: targets.map((t) => t.displayPath)
    };
  }

  async function confirmDiscard(): Promise<void> {
    if (!session || !discardConfirm || discardBusy) return;
    const current = session;
    const pending = discardConfirm;
    if (pending.kind === "all") {
      await confirmDiscardAll(current, pending.allPaths);
      return;
    }
    const displayPath = diff.selection?.path ?? statusFiles?.find((file) => file.pathId === pending.pathId)?.displayPath ?? "";
    discardBusy = true;
    discardError = null;
    indexError = null;
    try {
      session = pending.kind === "file"
        ? await statusAdapter().worktreeDiscardFile(current.repoId, current.version, pending.pathId, pending.token)
        : await statusAdapter().diffHunkDiscard(current.repoId, current.version, pending.pathId, pending.hunkId ?? "", pending.token);
      discardConfirm = null;
      if (pending.kind === "hunk" && displayPath) await refreshOpenWorktreeDiff(displayPath);
      else {
        clearDiff();
        await loadStatus();
      }
    } catch (error) {
      const appError = error as AppError;
      indexError = appError;
      discardError = `${appError.code}: ${appError.message}`;
      discardConfirm = null;
      await resyncMutationFailure(current);
      clearDiff();
    } finally {
      if (session?.repoId === current.repoId) discardBusy = false;
    }
  }

  /** Discard every unstaged file one by one, re-resolving fresh path ids
   * after each write (each discard bumps the status generation, so tokens
   * from the initial listing go stale). Stops at the first failure and
   * reports how many files were already discarded. */
  async function confirmDiscardAll(current: RepoSnapshot, displayPaths: string[]): Promise<void> {
    discardBusy = true;
    discardError = null;
    indexError = null;
    let done = 0;
    try {
      for (const displayPath of displayPaths) {
        if (!session || session.repoId !== current.repoId) {
          discardConfirm = null;
          return;
        }
        await loadStatus();
        const row = statusFiles?.find(
          (file) => file.displayPath === displayPath && !file.conflicted && file.worktreeStatus !== " " && file.worktreeStatus !== "!"
        );
        if (!row) {
          throw { code: "STALE_STATE", message: `The file list changed before '${displayPath}' could be discarded.`, recovery: "refresh", retryable: false } as AppError;
        }
        const live = session;
        const prepared = await statusAdapter().confirmationPrepare(live.repoId, live.version, "discard_file", [row.pathId]);
        session = await statusAdapter().worktreeDiscardFile(live.repoId, live.version, row.pathId, prepared.confirmationToken);
        done += 1;
      }
      discardConfirm = null;
      clearDiff();
      await loadStatus();
    } catch (error) {
      const appError = error as AppError;
      indexError = {
        ...appError,
        message: done > 0 ? `Discarded ${done} of ${displayPaths.length} files, then stopped: ${appError.message}` : appError.message
      };
      discardConfirm = null;
      await resyncMutationFailure(current);
      clearDiff();
    } finally {
      discardBusy = false;
    }
  }

  /** Effective identity line for the commit footer (read-only, no secrets). */
  async function loadIdentity(): Promise<void> {
    if (!session) {
      identityLabel = null;
      return;
    }
    const current = session;
    try {
      const identity = await statusAdapter().identityRead(current.repoId);
      if (session === null || session.repoId !== current.repoId) return;
      identityLabel =
        identity.name && identity.email
          ? `Author: ${identity.name} <${identity.email}> (${identity.scope})${identity.signing ? " · signing on" : ""}`
          : "Identity missing — set user.name and user.email in Git config to commit.";
    } catch {
      if (session?.repoId === current.repoId) identityLabel = null;
    }
  }

  /** Reload everything a ref/index mutation can move: refs, history, status. */
  async function reloadAfterMutation(): Promise<void> {
    await loadHistory(true);
    await loadStatus();
    await loadIdentity();
  }

  /**
   * Commit exactly the index. The draft (subject + body) survives every
   * failure and clears only on success; the selection clears with it.
   */
  async function commitSelected(): Promise<void> {
    if (!session || commitBusy) return;
    const current = session;
    const subject = shell.commitMessage;
    if (subject.trim() === "") return;
    commitBusy = true;
    commitError = null;
    try {
      const result = await statusAdapter().commitCreate(
        current.repoId,
        current.version,
        subject,
        commitBody
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      shell.commitMessage = "";
      commitBody = "";
      clearDiff();
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      commitError = e as AppError;
      // The backend version may have moved; re-sync the snapshot too.
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; rows still reload below.
        }
      }
      await loadStatus();
    } finally {
      if (session?.repoId === current.repoId) commitBusy = false;
    }
  }

  function openBranches(): void {
    openBranchesAt(null);
  }

  function openBranchesAt(startOid: string | null): void {
    branchError = null;
    deleteConfirm = null;
    branchStartOid = startOid;
    showBranches = true;
  }

  function closeBranches(): void {
    showBranches = false;
    branchStartOid = null;
  }

  const historyPlanRows = $derived.by(() => {
    if (!historyAction || historyAction.action !== "interactive-rebase") return [];
    const target = historyAction.row.oid;
    const index = historyRows.findIndex((row) => row.oid === target);
    if (index < 0) return [{ oid: historyAction.row.oid, subject: historyAction.row.subject }];
    return historyRows
      .slice(0, index + 1)
      .reverse()
      .map((row) => ({ oid: row.oid, subject: row.subject }));
  });

  function historyActionHead(rowOid: string): boolean {
    if (!session || session.head.kind === "unborn") return false;
    return session.head.oid === rowOid;
  }

  async function openHistoryAction(action: CommitActionId, row: CommitRow): Promise<void> {
    if (!session) return;
    const current = session;
    historyAction = { action, row };
    historyActionError = null;
    historyConfirmSummary = null;
    historyConfirmToken = null;
    const form = COMMIT_ACTION_FORMS[action];
    if (!form.confirmAction) return;
    historyActionBusy = true;
    try {
      const details = await statusAdapter().confirmationPrepare(
        current.repoId,
        current.version,
        form.confirmAction,
        [row.oid]
      );
      if (historyAction?.row.oid !== row.oid || session?.repoId !== current.repoId) return;
      historyConfirmSummary = details.summary;
      historyConfirmToken = details.confirmationToken;
    } catch (e) {
      if (session?.repoId !== current.repoId) return;
      historyActionError = e as AppError;
    } finally {
      historyActionBusy = false;
    }
  }

  function closeHistoryAction(): void {
    historyAction = null;
    historyActionError = null;
    historyConfirmSummary = null;
    historyConfirmToken = null;
  }

  function refCommitRow(ref: RefItem, subject: string): CommitRow {
    return {
      oid: ref.oid,
      parents: [],
      subject,
      authorName: "",
      authoredAt: "",
      committedAt: "",
      refs: [ref.refId],
      boundary: false
    };
  }

  async function branchAction(kind: BranchMenuAction, ref: RefItem): Promise<void> {
    if (!session) return;
    switch (kind) {
      case "checkout":
        if (ref.kind === "local") {
          await switchBranch(ref.refId);
        } else {
          // Remote checkout creates a local tracking branch and switches to
          // it (`git switch -c`); dirty worktrees go through the stash offer
          // inside switchBranch.
          const name = suggestedTrackName(ref.label);
          const localExists = refs.some((r) => r.kind === "local" && r.label === name);
          if (!localExists) {
            await switchBranch(ref.refId, name);
          } else {
            // The default name is taken locally (`git switch -c` would
            // refuse): open Branches prefilled so the user picks another
            // local name and Tracks that row instead.
            trackName = name;
            branchError = null;
            deleteConfirm = null;
            showBranches = true;
          }
        }
        return;
      case "merge":
        mergeError = null;
        mergeSource = ref.refId;
        showMerge = true;
        return;
      case "create-branch":
        openBranchesAt(ref.oid);
        return;
      case "reveal":
        await showInGraph(ref.oid);
        return;
      case "delete":
        showBranches = true;
        await askDeleteBranch(ref.refId);
        return;
      case "rebase":
        await openHistoryAction("rebase", refCommitRow(ref, ref.label));
        return;
      case "cherry-pick":
        await openHistoryAction("cherry-pick", refCommitRow(ref, `Tip of ${ref.label}`));
        return;
      case "revert":
        await openHistoryAction("revert", refCommitRow(ref, `Tip of ${ref.label}`));
        return;
      case "create-tag":
        await openHistoryAction("create-tag", refCommitRow(ref, ref.label));
        return;
      case "reset-soft":
        await openHistoryAction("reset-soft", refCommitRow(ref, ref.label));
        return;
      case "reset-mixed":
        await openHistoryAction("reset-mixed", refCommitRow(ref, ref.label));
        return;
      case "reset-hard":
        await openHistoryAction("reset-hard", refCommitRow(ref, ref.label));
        return;
      case "push-to":
        await openHistoryAction("push-to", refCommitRow(ref, ref.label));
        return;
      case "rename":
      case "upstream":
      case "move":
      case "push":
        await openBranchForm(kind, ref);
        return;
    }
  }

  function branchFormOverride(): BranchFormOverride | null {
    if (!branchForm) return null;
    const label = branchForm.ref.label;
    if (branchForm.kind === "rename") {
      return {
        title: "Rename branch",
        description: `Rename '${label}' to a new local name.`,
        fields: [{ kind: "text", key: "name", label: "New branch name", placeholder: label }],
        submitLabel: "Rename"
      };
    }
    if (branchForm.kind === "upstream") {
      return {
        title: "Set upstream",
        description: `Track a remote branch for '${label}'. Push then goes to that destination.`,
        fields: [
          {
            kind: "select",
            key: "upstreamRefId",
            label: "Upstream remote branch",
            options: refs
              .filter((r) => r.kind === "remote")
              .map((r) => ({ value: r.refId, label: r.label }))
          }
        ],
        submitLabel: "Set upstream"
      };
    }
    if (branchForm.kind === "move") {
      const short = (branchForm.targetOid ?? "").slice(0, 7);
      return {
        title: "Move branch",
        description: `Move '${label}' to ${short}. The branch will point at a different commit.`,
        fields: [],
        submitLabel: "Move branch",
        danger: true
      };
    }
    return {
      title: "Push branch",
      description: `Push '${label}' to its configured upstream (normal push, never force).`,
      fields: [],
      submitLabel: "Push"
    };
  }

  async function openBranchForm(
    kind: "rename" | "upstream" | "move" | "push",
    ref: RefItem
  ): Promise<void> {
    if (!session) return;
    const current = session;
    branchForm = { kind, ref, targetOid: shell.selectedCommitOid };
    historyActionError = null;
    historyConfirmSummary = null;
    historyConfirmToken = null;
    if (kind !== "move") return;
    const target = shell.selectedCommitOid;
    if (!target || target === ref.oid) return;
    historyActionBusy = true;
    try {
      const details = await statusAdapter().confirmationPrepare(
        current.repoId,
        current.version,
        "branch_move",
        [ref.refId, target]
      );
      if (branchForm?.ref.refId !== ref.refId || session?.repoId !== current.repoId) return;
      historyConfirmSummary = details.summary;
      historyConfirmToken = details.confirmationToken;
    } catch (e) {
      if (session?.repoId !== current.repoId) return;
      historyActionError = e as AppError;
    } finally {
      historyActionBusy = false;
    }
  }

  function closeBranchForm(): void {
    branchForm = null;
    historyActionError = null;
    historyConfirmSummary = null;
    historyConfirmToken = null;
  }

  async function submitBranchForm(values: Record<string, string>): Promise<void> {
    if (!session || !branchForm || historyActionBusy) return;
    const current = session;
    const form = branchForm;
    const adapter = statusAdapter();
    historyActionBusy = true;
    historyActionError = null;
    try {
      let snapshot: RepoSnapshot;
      if (form.kind === "rename") {
        snapshot = await adapter.branchRename(current.repoId, current.version, form.ref.refId, (values.name ?? "").trim());
      } else if (form.kind === "upstream") {
        snapshot = await adapter.branchSetUpstream(current.repoId, current.version, form.ref.refId, values.upstreamRefId ?? "");
      } else if (form.kind === "move") {
        if (!form.targetOid) throw { code: "INVALID_ARGUMENT", message: "Select a commit in the history first.", recovery: "inspectState", retryable: false } as AppError;
        snapshot = await adapter.branchMove(current.repoId, current.version, form.ref.refId, form.targetOid, historyToken());
      } else {
        snapshot = await adapter.branchPush(current.repoId, current.version, form.ref.refId);
      }
      if (session === null || session.repoId !== current.repoId) return;
      session = snapshot;
      closeBranchForm();
      clearDiff();
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      historyActionError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; rows still reload below.
        }
      }
      await loadStatus();
      await loadHistory(true);
    } finally {
      if (session?.repoId === current.repoId) historyActionBusy = false;
    }
  }

  function historyToken(): string {
    if (!historyConfirmToken) {
      throw { code: "INVALID_ARGUMENT", message: "The confirmation expired; close and reopen this dialog.", recovery: "inspectState", retryable: false } as AppError;
    }
    return historyConfirmToken;
  }

  async function submitHistoryAction(values: Record<string, string>, plan: RebasePlanEntry[]): Promise<void> {
    if (!session || !historyAction || historyActionBusy) return;
    const current = session;
    const { action, row } = historyAction;
    const adapter = statusAdapter();
    historyActionBusy = true;
    historyActionError = null;
    try {
      let snapshot: RepoSnapshot;
      switch (action) {
        case "checkout":
          if (shouldOfferStash()) {
            closeHistoryAction();
            openStashSwitch({ kind: "checkout", oid: row.oid, label: row.oid.slice(0, 7) });
            return;
          }
          snapshot = await adapter.historyCheckout(current.repoId, current.version, row.oid);
          break;
        case "create-branch":
          closeHistoryAction();
          openBranchesAt(row.oid);
          return;
        case "create-tag":
          snapshot = await adapter.tagCreateAt(current.repoId, current.version, row.oid, (values.name ?? "").trim());
          break;
        case "push-to":
          snapshot = await adapter.historyPushTo(current.repoId, current.version, row.oid, (values.remote ?? "").trim(), (values.destBranch ?? "").trim());
          break;
        case "cherry-pick":
          snapshot = await adapter.cherryPickOnto(current.repoId, current.version, row.oid);
          break;
        case "revert":
          snapshot = await adapter.revertCommit(current.repoId, current.version, row.oid);
          break;
        case "merge":
          snapshot = await adapter.mergeCommit(current.repoId, current.version, row.oid);
          break;
        case "rebase":
          snapshot = await adapter.rebaseOnto(current.repoId, current.version, row.oid, historyToken());
          break;
        case "reword":
          snapshot = await adapter.rewordMessage(current.repoId, current.version, row.oid, (values.subject ?? "").trim(), values.body ?? "");
          break;
        case "modify":
          snapshot = await adapter.modifyCommit(current.repoId, current.version, row.oid);
          break;
        case "edit-author":
          snapshot = await adapter.editAuthor(current.repoId, current.version, row.oid, (values.name ?? "").trim(), (values.email ?? "").trim());
          break;
        case "split":
          snapshot = await adapter.splitCommit(current.repoId, current.version, row.oid, historyToken());
          break;
        case "move-to-branch":
          snapshot = await adapter.moveToBranch(current.repoId, current.version, row.oid, (values.name ?? "").trim(), (values.switchAfter ?? "true") === "true");
          break;
        case "interactive-rebase":
          snapshot = await adapter.rebaseInteractiveFrom(current.repoId, current.version, row.oid, plan, historyToken());
          break;
        case "reset-soft":
          snapshot = await adapter.resetSoft(current.repoId, current.version, row.oid);
          break;
        case "reset-mixed":
          snapshot = await adapter.resetMixed(current.repoId, current.version, row.oid);
          break;
        case "reset-hard":
          snapshot = await adapter.resetHard(current.repoId, current.version, row.oid, historyToken());
          break;
      }
      if (session === null || session.repoId !== current.repoId) return;
      session = snapshot;
      closeHistoryAction();
      clearDiff();
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      historyActionError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; rows still reload below.
        }
      }
      await loadStatus();
      await loadHistory(true);
    } finally {
      if (session?.repoId === current.repoId) historyActionBusy = false;
    }
  }

  async function createBranch(): Promise<void> {
    if (!session || branchBusy) return;
    const current = session;
    if (current.head.kind === "unborn") {
      branchError = "No commits yet — create the first commit before branching.";
      return;
    }
    branchBusy = true;
    branchError = null;
    try {
      const result = await statusAdapter().branchCreate(
        current.repoId,
        current.version,
        newBranchName,
        branchStartOid ?? current.head.oid,
        switchAfterCreate
      );
      if (session === null || session.repoId !== current.repoId) return;
      session = result.snapshot;
      if (result.switched) {
        newBranchName = "";
      } else if (result.switchError) {
        branchError = `Branch created, but switch failed (${result.switchError.code}): ${result.switchError.message}`;
      }
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      branchError = e as AppError;
      await loadStatus();
    } finally {
      if (session?.repoId === current.repoId) branchBusy = false;
    }
  }

  function shouldOfferStash(): boolean {
    if (!session || session.trust !== "trusted") return false;
    return needsStashOffer(
      session.stagedCount ?? 0,
      session.unstagedCount ?? 0,
      session.conflictCount ?? 0
    );
  }

  function dirtySummary(): string {
    const staged = session?.stagedCount ?? 0;
    const unstaged = session?.unstagedCount ?? 0;
    const parts: string[] = [];
    if (staged > 0) parts.push(`${staged} staged`);
    if (unstaged > 0) parts.push(`${unstaged} unstaged`);
    return parts.join(", ") || "Uncommitted changes";
  }

  function openStashSwitch(target: StashSwitchTarget): void {
    stashSwitch = target;
    stashSwitchIncludeUntracked = true;
    stashSwitchError = null;
    stashSwitchDone = null;
  }

  function closeStashSwitch(): void {
    stashSwitch = null;
    stashSwitchError = null;
    stashSwitchDone = null;
  }

  async function confirmStashSwitch(): Promise<void> {
    if (!session || !stashSwitch || stashSwitchBusy || branchBusy) return;
    const current = session;
    const target = stashSwitch;
    stashSwitchBusy = true;
    stashSwitchError = null;
    try {
      const stash = await statusAdapter().stashSave(
        current.repoId,
        current.version,
        autoStashMessage(target.label),
        stashSwitchIncludeUntracked
      );
      let snapshot: RepoSnapshot;
      if (target.kind === "branch") {
        snapshot = await statusAdapter().branchSwitch(
          current.repoId,
          stash.snapshot.version,
          target.refId,
          target.trackName
        );
      } else {
        snapshot = await statusAdapter().historyCheckout(
          current.repoId,
          stash.snapshot.version,
          target.oid
        );
      }
      if (session === null || session.repoId !== current.repoId) return;
      session = snapshot;
      stashSwitchDone = stash.noChange
        ? `Switched to ${target.label}. Nothing needed stashing.`
        : `Switched to ${target.label}. Changes stashed — restore them from Stashes when ready.`;
      clearDiff();
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      stashSwitchError = e as AppError;
      if (!demo) {
        try {
          session = await realAdapter.repoSnapshot(current.repoId, false);
        } catch {
          // Keep the last snapshot; rows still reload below.
        }
      }
      await loadStatus();
      await loadHistory(true);
    } finally {
      if (session?.repoId === current.repoId) stashSwitchBusy = false;
    }
  }

  async function switchBranch(refId: string, trackAs: string | null = null): Promise<void> {
    if (!session || branchBusy) return;
    const current = session;
    if (shouldOfferStash()) {
      const target = refs.find((r) => r.refId === refId);
      openStashSwitch({ kind: "branch", refId, label: target?.label ?? refId, trackName: trackAs });
      return;
    }
    branchBusy = true;
    branchError = null;
    try {
      session = await statusAdapter().branchSwitch(current.repoId, current.version, refId, trackAs);
      if (session === null || session.repoId !== current.repoId) return;
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      branchError = e as AppError;
      await loadStatus();
    } finally {
      if (session?.repoId === current.repoId) branchBusy = false;
    }
  }

  async function trackBranch(refId: string): Promise<void> {
    if (!session || branchBusy || trackName.trim() === "") return;
    const name = trackName.trim();
    await switchBranch(refId, name);
    if (session && !stashSwitch && !branchError) trackName = "";
  }

  async function askDeleteBranch(refId: string): Promise<void> {
    if (!session || branchBusy) return;
    const current = session;
    branchBusy = true;
    branchError = null;
    try {
      const details = await statusAdapter().confirmationPrepare(
        current.repoId,
        current.version,
        "branch_delete",
        [refId]
      );
      if (session === null || session.repoId !== current.repoId) return;
      deleteConfirm = {
        refId,
        summary: details.summary,
        token: details.confirmationToken
      };
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      branchError = e as AppError;
    } finally {
      if (session?.repoId === current.repoId) branchBusy = false;
    }
  }

  async function confirmDeleteBranch(): Promise<void> {
    if (!session || branchBusy || !deleteConfirm) return;
    const current = session;
    const target = deleteConfirm;
    branchBusy = true;
    branchError = null;
    try {
      session = await statusAdapter().branchDelete(
        current.repoId,
        current.version,
        target.refId,
        target.token
      );
      if (session === null || session.repoId !== current.repoId) return;
      deleteConfirm = null;
      await reloadAfterMutation();
    } catch (e) {
      if (session === null || session.repoId !== current.repoId) return;
      branchError = e as AppError;
      await loadStatus();
    } finally {
      if (session?.repoId === current.repoId) branchBusy = false;
    }
  }

  async function applyScope(value: string): Promise<void> {
    clearDiff();
    scopeValue = value;
    scope = value === "all" ? { ...ALL_REFS_SCOPE } : value === "head" ? { type: "head" } : { type: "ref", refId: value.slice(4) };
    searchController.invalidate();
    if (!session) return;
    if (searchQuery.trim() !== "") await runSearch(session.repoId, searchQuery, true);
    else await loadHistory(true);
  }

  async function showInGraph(oid: string): Promise<void> {
    // Leave search mode, show the full topology, then select the commit.
    searchQuery = "";
    lastScheduledQuery = "";
    resetSearch();
    scope = { ...ALL_REFS_SCOPE };
    scopeValue = "all";
    await loadHistory(true);
    selectCommit(oid);
  }

  async function loadPreflight(): Promise<void> {
    preflight = null;
    try {
      preflight = demo
        ? await mockAdapter.appPreflight({ requestId: newRequestId() })
        : await realAdapter.appPreflight();
    } catch (e) {
      sessionError = e as AppError;
    }
  }

  async function trustRepo(): Promise<void> {
    if (!session) return;
    busy = true;
    try {
      session = demo
        ? await mockAdapter.repoTrustSet(session.repoId, true)
        : await realAdapter.repoTrustSet(session.repoId, true);
      // Trust unlocks worktree reads; fetch the first listing right away.
      await loadStatus();
      await loadRemoteStatus();
      await loadIdentity();
    } catch (e) {
      sessionError = e as AppError;
    } finally {
      busy = false;
    }
  }

  async function refreshAll(): Promise<void> {
    if (session && !demo) {
      try {
        session = await realAdapter.repoSnapshot(session.repoId, true);
        await loadStatus();
        await loadRemoteStatus();
      } catch (e) {
        sessionError = e as AppError;
      }
    }
    await loadPreflight();
  }

  function selectCommit(oid: string): void {
    shell.selectedCommitOid = oid;
    shell.inspector = "commit";
    clearDiff();
    void loadDetails(oid, null);
  }

  function changeInspector(state: InspectorState): void {
    if (shell.inspector !== state) clearDiff();
    shell.inspector = state;
  }

  function selectSection(id: string): void {
    section = id;
    if (id === "working") {
      clearDiff();
      changeInspector("working");
    }
  }

  function selectRef(refId: string): void {
    const kind = refs.find((r) => r.refId === refId)?.kind;
    section = kind === "remote" ? "remote" : kind === "tag" ? "tags" : "local";
    void applyScope(`ref:${refId}`);
  }

  function resizeSidebar(delta: number): void {
    shell.sidebarWidth = clampWidth(
      shell.sidebarWidth + delta,
      SHELL_LIMITS.sidebarMin,
      SHELL_LIMITS.sidebarMax
    );
    persistWidths();
  }

  function resizeInspector(delta: number): void {
    shell.inspectorWidth = clampWidth(
      shell.inspectorWidth - delta,
      SHELL_LIMITS.inspectorMin,
      SHELL_LIMITS.inspectorMax
    );
    persistWidths();
  }

  function globalKeyDown(e: KeyboardEvent): void {
    if (!active || e.defaultPrevented) return;
    if (e.key === "Escape" && discardConfirm) {
      e.preventDefault();
      if (!discardBusy) {
        discardConfirm = null;
        discardError = null;
      }
      return;
    }
    // Shortcuts never fire while typing; Esc just leaves the field —
    // dialogs and inputs handle their own keys.
    const target = e.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable)
    ) {
      if (e.key === "Escape") target.blur();
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "f") {
      e.preventDefault();
      topBar?.focusSearch();
    } else if (mod && e.key.toLowerCase() === "o") {
      e.preventDefault();
      onOpenRepository();
    } else if (mod && e.key.toLowerCase() === "r") {
      e.preventDefault();
      void refreshAll();
    } else if (e.key === "Escape") {
      if (diff.selection) {
        e.preventDefault();
        closeDiff();
      } else if (shell.inspector !== "working") changeInspector("working");
    }
  }

  const selectionSummary = $derived.by(() => {
    const current: RepoSnapshot | null = session;
    if (current === null) return "No repository open";
    return `${current.displayName} · ${historyRows.length} commits · inspector: ${shell.inspector}`;
  });

  // Restore before autosave can write an empty initial draft.
  restoreDraft(untrack(() => initialSession.workspaceKey));
  let disposed = false;
  onMount(() => {
    void loadPreflight();
    void loadSettings();
    restoreWidths();
    void Promise.all([loadHistory(true), loadStatus(), loadRemoteStatus(), loadIdentity()]);
    void subscribeInvalidated();
  });
  onDestroy(() => {
    disposed = true;
    persistDraft();
    clearTimeout(searchTimer);
    searchController.invalidate();
    detailsRequest += 1;
    clearDiff();
    stopSyncPoll();
    unsubscribeInvalidated();
  });
  $effect(() => {
    if (active) untrack(() => { if (session && !statusLoading) void refreshActivatedRepository(); });
  });
  $effect(() => {
    if (!session) return;
    const next: WorkspaceState = {
      snapshot: session,
      busy: busy || indexBusy || discardBusy || commitBusy || branchBusy || syncJobActive || mergeBusy || conflictBusy !== null || stashSaveBusy || stashBusyEntry !== null,
      hasDraft: !!(shell.commitMessage.trim() || commitBody.trim()),
      changedFiles: statusFiles?.length ?? null,
      hasError: !!(sessionError || indexError || commitError || syncError || conflictActionError),
      modalOpen: showBranches || showMerge || showStash || showSettings || showHelp || discardConfirm !== null || branchForm !== null || stashSwitch !== null
    };
    untrack(() => onWorkspaceChange(next));
  });

  // Draft autosave uses a stable, backend-issued worktree key.
  // Switching repositories restores each repo's own draft on open.
  $effect(() => {
    shell.commitMessage;
    commitBody;
    persistDraft();
  });

  // Debounced search: every keystroke schedules, only the latest commits.
  // `lastScheduledQuery` is plain (untracked) state so result commits below
  // never reschedule the timer.
  $effect(() => {
    const q = searchQuery;
    const current = session;
    if (q === lastScheduledQuery) return;
    lastScheduledQuery = q;
    clearTimeout(searchTimer);
    if (q.trim() === "" || current === null) {
      resetSearch();
      return;
    }
    searchError = null;
    searchLoading = true;
    const repoId = current.repoId;
    searchTimer = setTimeout(() => {
      void runSearch(repoId, q, true);
    }, SEARCH_DEBOUNCE_MS);
  });
</script>

<svelte:window onkeydown={globalKeyDown} onfocus={handleFocus} />

<div class="gd-shell">
  {#if session}
    <TopBar
      bind:this={topBar}
      {demo}
      branch={headLabel(session.head)}
      {searchQuery}
      {identityLabel}
      onSearchInput={(value) => (searchQuery = value)}
      onSearchFocus={() => undefined}
      onOpen={onOpenRepository}
      onInit={onInitRepository}
      onClose={onCloseRepository}
      onBranches={openBranches}
      onOpenSettings={() => (showSettings = true)}
      onOpenHelp={() => (showHelp = true)}
      repoName={session.displayName}
    >
      {#snippet actions()}
        <GitActions
          onBranches={() => openBranches()}
          syncDisabled={syncDisabled}
          {syncDisabledReason}
          jobActive={syncJobActive}
          activeJobKind={syncJobActive ? (syncJob?.kind ?? null) : null}
          onFetch={() => void startSyncJob("fetch")}
          onPull={() => void startSyncJob("pull")}
          onPush={() => void startSyncJob("push")}
          onMerge={openMerge}
          onStash={openStash}
          onCancel={() => void cancelSyncJob()}
          {bitbucketAvailable}
          onConnectBitbucket={openBitbucketAuth}
          logOpen={syncLogOpen}
          onToggleLog={toggleSyncLog}
        />
      {/snippet}
    </TopBar>
    <GitToolbar
      onBranches={() => openBranches()}
      {remoteLabel}
      {remoteTitle}
      {syncDisabled}
      {syncDisabledReason}
      jobActive={syncJobActive}
      jobLabel={syncJobLabel}
      jobProgress={syncJob?.progress ?? null}
      onFetch={() => void startSyncJob("fetch")}
      onPull={() => void startSyncJob("pull")}
      onPush={() => void startSyncJob("push")}
      onMerge={openMerge}
      onStash={openStash}
      onCancel={() => void cancelSyncJob()}
      {syncError}
      {syncErrorCode}
      syncRetryLabel={syncRetryKind ? `Retry ${syncRetryKind}` : null}
      onRetrySync={() => { if (syncRetryKind) void startSyncJob(syncRetryKind); }}
      {bitbucketAvailable}
      onConnectBitbucket={openBitbucketAuth}
      logOpen={syncLogOpen}
      onToggleLog={toggleSyncLog}
      logLoading={syncLogLoading}
      logEntries={syncLogEntries}
      logError={syncLogError}
    />
    {#if showBitbucketAuth && remoteStatus?.url}
      <BitbucketAuthModal
        remoteUrl={remoteStatus.url}
        token={bitbucketToken}
        busy={bitbucketBusy}
        error={bitbucketError}
        retryKind={bitbucketRetryKind}
        onToken={(value) => (bitbucketToken = value)}
        onCreateToken={() => void openBitbucketTokenPage()}
        onSubmit={() => void connectBitbucket()}
        onClose={closeBitbucketAuth}
      />
    {/if}
    {#if session.trust === "readOnly"}
      <div class="gd-trust-bar" role="note">
        <span>
          {session.displayName} is open read-only. History is available; working
          changes need trust because local hooks and filters may run.
        </span>
        <button type="button" onclick={() => void trustRepo()} disabled={busy}>
          Trust this repository
        </button>
      </div>
    {/if}
    <div class="gd-content">
      <Sidebar
        width={shell.sidebarWidth}
        {refs}
        starScope={session.workspaceKey}
        activeSection={section}
        {activeRefId}
        actionsDisabled={busy || branchBusy || session?.trust !== "trusted"}
        selectedCommitOid={shell.selectedCommitOid}
        onSelect={selectSection}
        onRefSelect={selectRef}
        onBranchAction={(action, ref) => void branchAction(action, ref)}
      />
      <Splitter
        label="Resize sidebar"
        onResize={resizeSidebar}
        onReset={() => (shell.sidebarWidth = SHELL_LIMITS.sidebarDefault)}
      />
      <main class="gd-main" aria-label="Main panel">
      <div class="gd-history-slot" hidden={diff.selection !== null}>
      {#if workBar}
        <button
          type="button"
          class="gd-workbar"
          title="Open working changes"
          aria-label={`${workBar.total} uncommitted changes. Open working changes.`}
          onclick={() => changeInspector("working")}
        >
          <strong>{workBar.total} changed</strong>
          <span class="gd-workbar-stats">
            {#if workBar.added}<span class="gd-added">+{workBar.added} added</span>{/if}
            {#if workBar.modified}<span class="gd-modified">{workBar.modified} modified</span>{/if}
            {#if workBar.deleted}<span class="gd-deleted">−{workBar.deleted} deleted</span>{/if}
          </span>
          <span class="gd-workbar-go" aria-hidden="true">→</span>
        </button>
      {/if}
      <HistoryPane
        rows={historyRows}
        laid={laidRows}
        {laneCount}
        loading={historyLoading}
        loadingMore={historyLoadingMore}
        error={historyError}
        hasMore={historyHasMore}
        pageTruncated={historyTruncated}
        totalHint={`${historyRows.length} loaded${historyHasMore ? ", more available" : ""}`}
        {emptyHint}
        selectedOid={shell.selectedCommitOid}
        {refLabels}
        {refs}
        {scopeValue}
        {scopeOptions}
        onScopeChange={(value) => void applyScope(value)}
        {searchActive}
        {searchQuery}
        {searchRows}
        {searchLoading}
        {searchLoadingMore}
        {searchError}
        {searchHasMore}
        {searchIncomplete}
        onSearchLoadMore={() => {
          if (session) void runSearch(session.repoId, searchQuery, false);
        }}
        onSearchRetry={() => {
          if (session) void runSearch(session.repoId, searchQuery, true);
        }}
        onShowInGraph={(oid) => void showInGraph(oid)}
        onClearSearch={() => {
          searchQuery = "";
        }}
        onCreateBranch={openBranchesAt}
        onCommitAction={(action, row) => void openHistoryAction(action, row)}
        onSelect={selectCommit}
        onRetry={() => void loadHistory(true)}
        onLoadMore={() => void loadHistory(false)}
      />
      </div>
      {#if diff.selection}
        {#key diff.selection}
          <DiffPane
            state={diff}
            onClose={closeDiff}
            onRetry={() => void diffController.retry()}
            {canMutateHunks}
            {hunkMutationHint}
            mutationBusy={indexBusy || discardBusy}
            mutationError={indexError ? `${indexError.code}: ${indexError.message}` : null}
            onStageHunk={(hunkId) => void stageHunk(hunkId)}
            onDiscardHunk={(hunkId) => void askDiscardHunk(hunkId)}
          />
        {/key}
      {/if}
      </main>
      <Splitter
        label="Resize inspector"
        onResize={resizeInspector}
        onReset={() => (shell.inspectorWidth = SHELL_LIMITS.inspectorDefault)}
      />
      <Inspector
        width={shell.inspectorWidth}
        state={shell.inspector satisfies InspectorState}
        branchName={headLabel(session.head)}
        {refs}
        onParentCommit={selectCommit}
        commitMessage={shell.commitMessage}
        {commitDetails}
        {detailsLoading}
        {detailsError}
        {statusFiles}
        {statusLoading}
        {statusError}
        trustBlocked={session.trust === "readOnly"}
        onRefreshStatus={() => void loadStatus()}
        selectedDiffTarget={diff.selection?.target ?? null}
        onWorktreeDiff={loadWorktreeDiff}
        onCommitDiff={loadCommitDiff}
        {indexBusy}
        {indexError}
        onStageFiles={(ids) => void mutateIndex(true, ids)}
        onUnstageFiles={(ids) => void mutateIndex(false, ids)}
        onDiscardFile={(id) => void askDiscardFile(id)}
        onDiscardAllFiles={() => askDiscardAll()}
        {commitBody}
        {commitBusy}
        {commitError}
        {identityLabel}
        headDetached={session.head.kind === "detached"}
        onCommitBody={(value) => (commitBody = value)}
        onCommit={() => void commitSelected()}
        onStateChange={changeInspector}
        onCommitMessage={(value) => (shell.commitMessage = value)}
        onParentChange={(parentIndex) => {
          // Parent switch re-issues every file token: drop the old diff.
          clearDiff();
          if (shell.selectedCommitOid) void loadDetails(shell.selectedCommitOid, parentIndex);
        }}
        onRetryDetails={() => {
          if (shell.selectedCommitOid) void loadDetails(shell.selectedCommitOid, commitDetails?.parentIndex ?? null);
        }}
        {mergeBanner}
        {conflictFiles}
        conflictFilesLoading={conflictLoading}
        conflictFilesError={conflictError}
        conflictSelected={conflictSelected}
        conflictPreview={conflictPreview}
        {conflictPreviewLoading}
        {conflictPreviewError}
        conflictBusy={conflictBusy}
        {conflictActionError}
        {conflictNotice}
        {mergeSubject}
        {reviewedStaged}
        {canComplete}
        {canAbort}
        {abortReason}
        {abortConfirm}
        {acceptConfirm}
        onSelectConflict={(pathId) => selectConflict(pathId)}
        onReloadConflicts={() => void loadConflicts()}
        onAskAccept={(side) => void askAccept(side)}
        onConfirmAccept={() => void confirmAccept()}
        onCancelAccept={() => (acceptConfirm = null)}
        onMarkWorking={() => void markResolved("workingFile")}
        onMarkDeletion={() => void markResolved("deletion")}
        onMergeSubject={(value) => (mergeSubject = value)}
        onCompleteMerge={() => void completeMerge()}
        onAskAbort={() => void askAbort()}
        onConfirmAbort={() => void confirmAbort()}
        onCancelAbort={() => {
          abortConfirm = null;
          abortConfirmToken.current = null;
        }}
      />
    </div>
    {#if sessionError}
      <p class="gd-session-error" role="alert">
        {sessionError.code}: {sessionError.message}
      </p>
    {/if}
    <StatusBar
      operation={sessionError ? `Error (${sessionError.code})` : "Idle"}
      {selectionSummary}
      gitVersion={preflight?.gitVersion ?? null}
    />
  {/if}
  {#if active && discardConfirm}
    <DiscardConfirmModal
      summary={discardConfirm.summary}
      busy={discardBusy}
      error={discardError}
      onConfirm={() => void confirmDiscard()}
      onCancel={() => {
        discardConfirm = null;
        discardError = null;
      }}
    />
  {/if}
  {#if active && showBranches && session}
    <BranchModal
      localRefs={refs.filter((r) => r.kind === "local")}
      remoteRefs={refs.filter((r) => r.kind !== "local")}
      startOidShort={branchStartOid
        ? branchStartOid.slice(0, 12)
        : session.head.kind === "unborn" ? null : session.head.oid.slice(0, 12)}
      startSource={branchStartOid ? "commit" : "HEAD"}
      newName={newBranchName}
      switchAfter={switchAfterCreate}
      {trackName}
      busy={branchBusy}
      error={branchError}
      deleteConfirm={deleteConfirm
        ? { refId: deleteConfirm.refId, summary: deleteConfirm.summary }
        : null}
      onName={(value) => (newBranchName = value)}
      onSwitchAfter={(value) => (switchAfterCreate = value)}
      onTrackName={(value) => (trackName = value)}
      onCreate={() => void createBranch()}
      onSwitch={(refId) => void switchBranch(refId)}
      onAskDelete={(refId) => void askDeleteBranch(refId)}
      onConfirmDelete={() => void confirmDeleteBranch()}
      onCancelDelete={() => (deleteConfirm = null)}
      onTrack={(refId) => {
        const target = refs.find((r) => r.refId === refId);
        if (target && trackName.trim() === "") {
          trackName = target.label.includes("/") ? target.label.split("/").slice(1).join("/") : target.label;
        }
        void trackBranch(refId);
      }}
      onClose={closeBranches}
    />
  {/if}
  {#if active && historyAction && session}
    <CommitActionModal
      action={historyAction.action}
      oid={historyAction.row.oid}
      subject={historyAction.row.subject}
      isHead={historyActionHead(historyAction.row.oid)}
      busy={historyActionBusy}
      error={historyActionError}
      confirmSummary={historyConfirmSummary}
      planRows={historyPlanRows}
      onSubmit={(values, plan) => void submitHistoryAction(values, plan)}
      onClose={closeHistoryAction}
    />
  {/if}
  {#if active && stashSwitch && session}
    <StashSwitchModal
      targetLabel={stashSwitch.label}
      dirtySummary={dirtySummary()}
      includeUntracked={stashSwitchIncludeUntracked}
      busy={stashSwitchBusy}
      error={stashSwitchError}
      done={stashSwitchDone}
      onIncludeUntracked={(value) => (stashSwitchIncludeUntracked = value)}
      onConfirm={() => void confirmStashSwitch()}
      onClose={closeStashSwitch}
    />
  {/if}
  {#if active && branchForm && session}
    <CommitActionModal
      action="branch-form"
      oid={branchForm.targetOid ?? branchForm.ref.oid}
      subject={branchForm.ref.label}
      isHead={false}
      busy={historyActionBusy}
      error={historyActionError}
      confirmSummary={historyConfirmSummary}
      planRows={[]}
      formOverride={branchFormOverride()}
      onSubmit={(values) => void submitBranchForm(values)}
      onClose={closeBranchForm}
    />
  {/if}
  {#if active && showSettings}
    <SettingsModal
      fontScale={settingsDraftScale}
      version={settings?.version ?? null}
      busy={settingsBusy}
      error={settingsError}
      onScale={(value) => {
        settingsDraftScale = Math.min(1.25, Math.max(0.875, value));
        applyFontScale(settingsDraftScale);
      }}
      onSave={() => void saveSettings()}
      onClose={() => {
        showSettings = false;
        settingsError = null;
        if (settings) settingsDraftScale = settings.fontScale;
      }}
    />
  {/if}
  {#if active && showHelp}
    <HelpModal
      gitVersion={preflight?.gitVersion ?? null}
      platform={preflight?.platform ?? null}
      mode={demo ? "Demo data (browser)" : "Native"}
      onClose={() => (showHelp = false)}
    />
  {/if}
  {#if active && showMerge && session}
    <MergeModal
      sources={mergeSources}
      targetLabel={mergeTargetLabel}
      targetOidShort={mergeTargetOid?.slice(0, 12) ?? null}
      selectedSource={mergeSource}
      busy={mergeBusy}
      error={mergeError}
      canStart={mergeSource !== "" && mergeTargetOid !== null && !syncDisabled}
      startHint={syncDisabled
        ? syncDisabledReason
        : mergeTargetOid === null
          ? "Merges start from a branch HEAD"
          : mergeSource === ""
            ? "No other branch to merge from"
            : `Merge into ${mergeTargetLabel} with a review stop`}
      onSelectSource={(value) => (mergeSource = value)}
      onStart={() => void startMerge()}
      onClose={() => (showMerge = false)}
    />
  {/if}
  {#if active && showStash && session}
    <StashModal
      entries={stashEntries}
      loading={stashLoading}
      error={stashError}
      notice={stashNotice}
      message={stashMessage}
      includeUntracked={stashIncludeUntracked}
      saveBusy={stashSaveBusy}
      busyEntry={stashBusyEntry}
      saveDisabledReason={syncDisabled ? syncDisabledReason : null}
      onMessage={(value) => (stashMessage = value)}
      onIncludeUntracked={(value) => (stashIncludeUntracked = value)}
      onSave={() => void saveStash()}
      onApply={(entry) => void applyStash(entry, "apply")}
      onPop={(entry) => void applyStash(entry, "pop")}
      onClose={closeStash}
    />
  {/if}
</div>

<style>
  .gd-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
    background: var(--gd-canvas);
    color: var(--gd-text);
    font-family: var(--gd-font-ui);
    font-size: var(--gd-font-size);
  }
  .gd-content {
    flex: 1 1 auto;
    display: flex;
    min-height: 0;
  }
  .gd-main {
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .gd-history-slot { display: flex; flex-direction: column; height: 100%; }
  .gd-history-slot[hidden] { display: none; }
  .gd-workbar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--gd-space-3);
    flex: 0 0 auto;
    min-height: 28px;
    padding: 4px var(--gd-space-3);
    color: var(--gd-text);
    background: var(--gd-surface-raised);
    border: 0;
    border-bottom: 1px solid var(--gd-border);
    cursor: pointer;
    font-size: var(--gd-font-size-small);
    text-align: left;
  }
  .gd-workbar:hover { background: var(--gd-surface-hover); }
  .gd-workbar:focus-visible { outline: 2px solid var(--gd-focus); outline-offset: -2px; }
  .gd-workbar-stats { display: flex; gap: var(--gd-space-2); color: var(--gd-text-secondary); }
  .gd-workbar .gd-added { color: var(--gd-accent); }
  .gd-workbar .gd-modified { color: var(--gd-warning); }
  .gd-workbar .gd-deleted { color: var(--gd-danger); }
  .gd-workbar-go { color: var(--gd-text-secondary); }
  .gd-trust-bar {
    display: flex;
    align-items: center;
    gap: var(--gd-space-3);
    padding: var(--gd-space-2) var(--gd-space-3);
    background: var(--gd-surface-raised);
    border-bottom: 1px solid var(--gd-border);
    font-size: var(--gd-font-size-small);
    flex: 0 0 auto;
  }
  .gd-trust-bar button {
    padding: 5px 12px;
    color: var(--gd-on-accent);
    background: var(--gd-accent);
    border: 0;
    border-radius: var(--gd-radius-control);
    cursor: pointer;
    white-space: nowrap;
  }
  .gd-trust-bar button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .gd-trust-bar button:focus-visible {
    outline: 2px solid var(--gd-focus);
    outline-offset: 1px;
  }
  .gd-session-error {
    margin: 0;
    padding: var(--gd-space-2) var(--gd-space-3);
    color: var(--gd-danger);
    font-size: var(--gd-font-size-small);
    border-top: 1px solid var(--gd-border);
  }
</style>
