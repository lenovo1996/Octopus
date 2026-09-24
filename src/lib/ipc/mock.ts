import { demoPreflight } from "../../mocks/demoPreflight";
import { demoCommits } from "../../mocks/demoRepo";
import { demoRecents, demoSession as initialDemoSession } from "../../mocks/demoSession";
import type {
  AppError,
  AppPreflightRequest,
  BitbucketConnectionResult,
  BranchCreateResult,
  ChangedFile,
  CommitDetails,
  CommitResult,
  CommitRow,
  ConfirmationDetails,
  DiffDocument,
  DiffTarget,
  HistoryPage,
  HistoryScope,
  IdentityInfo,
  ConflictAcceptResult,
  ConflictFile,
  ConflictList,
  ConflictPreview,
  MergeCompleteResult,
  MergeStartResult,
  SettingsV1,
  OperationLogEntry,
  OperationLogPage,
  OperationRecord,
  OperationStarted,
  PreflightData,
  RemoteStatus,
  StashApplyResult,
  StashEntry,
  StashSaveResult,
  RecentEntry,
  RefItem,
  RepoSnapshot,
  SearchResults,
  StatusData,
  WorkspacesRestoreResult,
  WorkspacesSaved
} from "./types";

function demoRows(): CommitRow[] {
  return demoCommits.map((c) => ({
    oid: c.oid,
    parents: c.parents,
    subject: c.subject,
    authorName: c.authorName,
    committedAt: c.committedAt,
    authoredAt: c.committedAt,
    refs: [...c.refs],
    boundary: false
  }));
}

let mockSettings: SettingsV1 = { version: 1, fontScale: 1 };

/**
 * Explicit browser demo / test adapter. Shares the same DTOs as the real
 * client but never touches native APIs. The UI must label it "Demo data".
 */
export function createMockAdapter(seed: RepoSnapshot = initialDemoSession) {
const demoSession = structuredClone(seed);
const mockAdapter = {
  adapter: "mock" as const,
  async appPreflight(_request: AppPreflightRequest): Promise<PreflightData> {
    await new Promise((resolve) => setTimeout(resolve, 50));
    return { ...demoPreflight };
  },
  async repoOpen(_selectedPath: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 50));
    mockWorktree(demoSession.repoId);
    return mockSessionWithCounts();
  },
  async repoInit(_selectedPath: string, initialBranch: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 50));
    demoSession.head = { kind: "unborn", name: initialBranch };
    return structuredClone(demoSession);
  },
  async repoSnapshot(): Promise<RepoSnapshot> {
    return mockSessionWithCounts();
  },
  async repoTrustSet(_repoId: string, trusted: boolean): Promise<RepoSnapshot> {
    const session = structuredClone(demoSession);
    session.trust = trusted ? "trusted" : "readOnly";
    return session;
  },
  async repoClose(): Promise<{ closed: boolean }> {
    return { closed: true };
  },
  async repoRecentList(): Promise<RecentEntry[]> {
    return structuredClone(demoRecents);
  },
  async repoRecentRemove(_entryId: string): Promise<{ removed: boolean }> {
    return { removed: true };
  },
  async repoRefs(): Promise<RefItem[]> {
    return structuredClone(mockBranches());
  },
  async identityRead(_repoId: string): Promise<IdentityInfo> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { name: "Demo Author", email: "demo@example.com", scope: "local", signing: false };
  },
  async commitCreate(repoId: string, _expectedVersion: number, subject: string, _body: string): Promise<CommitResult> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (subject.trim() === "") {
      const error: AppError = { code: "INVALID_ARGUMENT", message: "Commit subject cannot be empty.", recovery: "inspectState", retryable: false };
      throw error;
    }
    const rows = mockWorktree(repoId);
    if (!rows.some((f) => ![" ", "?", "!"].includes(f.indexStatus))) {
      const error: AppError = { code: "EMPTY_INDEX", message: "Nothing is staged.", recovery: "inspectState", retryable: false };
      throw error;
    }
    mockCommitCounter += 1;
    const oid = mockCommitCounter.toString(16).padStart(40, "a");
    // The commit consumes the index: staged-only rows vanish, staged AND
    // unstaged rows keep their worktree half.
    mockWorktreeByRepo.set(
      repoId,
      rows.flatMap((f) => {
        if ([" ", "?", "!"].includes(f.indexStatus)) return [f];
        if (![" ", "!"].includes(f.worktreeStatus)) return [{ ...f, indexStatus: " " }];
        return [];
      })
    );
    return { oid, snapshot: mockSessionWithCounts() };
  },
  async branchCreate(repoId: string, _expectedVersion: number, name: string, startOid: string, switchAfterCreate: boolean): Promise<BranchCreateResult> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const branches = mockBranches();
    const trimmed = name.trim();
    if (trimmed === "" || trimmed.startsWith("-")) {
      throw staleMockError("Branch name is empty or option-like.");
    }
    if (branches.some((b) => b.label === trimmed && b.kind === "local")) {
      throw staleMockError("A branch with this name already exists.");
    }
    const fullName = `refs/heads/${trimmed}`;
    const created: RefItem = { refId: fullName, fullName, label: trimmed, kind: "local", oid: startOid, current: false, checkedOutElsewhere: false };
    branches.push(created);
    let switched = false;
    if (switchAfterCreate) {
      for (const b of branches) b.current = b.refId === fullName;
      switched = true;
    }
    void repoId;
    return { snapshot: mockSessionWithCounts(), switched };
  },
  async branchSwitch(_repoId: string, _expectedVersion: number, refId: string, _newLocalName?: string | null): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const branches = mockBranches();
    const target = branches.find((b) => b.refId === refId && b.kind === "local");
    if (!target) throw staleMockError("Unknown branch reference.");
    for (const b of branches) b.current = b.refId === refId;
    return mockSessionWithCounts();
  },
  async confirmationPrepare(repoId: string, _expectedVersion: number, action: string, targets: string[]): Promise<ConfirmationDetails> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    mockConfirmCounter += 1;
    const token = `mock-token-${mockConfirmCounter}`;
    if (action === "discard_file") {
      if (targets.length !== 1) throw staleMockError("Discard file confirms one file.");
      const file = mockWorktree(repoId).find((row) => row.pathId === targets[0]);
      if (!file) throw staleMockError("The file list changed.");
      mockConfirmTokens.set(token, `discard_file:${targets[0]}`);
      return { confirmationToken: token, summary: `Discard all unstaged changes in '${file.displayPath}'. This cannot be undone by GitDock.`, expiresAt: Date.now() + 60000 };
    }
    if (action === "discard_hunk") {
      if (targets.length !== 2) throw staleMockError("Discard hunk confirms one hunk.");
      const file = mockWorktree(repoId).find((row) => row.pathId === targets[0]);
      if (!file) throw staleMockError("The file list changed.");
      mockConfirmTokens.set(token, `discard_hunk:${targets.join("\0")}`);
      return { confirmationToken: token, summary: `Discard this hunk from '${file.displayPath}' and restore its lines from the index. This cannot be undone by GitDock.`, expiresAt: Date.now() + 60000 };
    }
    if (action === "conflict_accept") {
      if (targets.length !== 2) throw staleMockError("Conflict accept confirms one file and one side.");
      mockConfirmTokens.set(token, targets.join("\0"));
      return { confirmationToken: token, summary: `Overwrite working file '${targets[0]}' with the ${targets[1]} version. The current working content is lost; the file stays unstaged.`, expiresAt: Date.now() + 60000 };
    }
    if (action === "merge_abort") {
      mockConfirmTokens.set(token, "merge");
      return { confirmationToken: token, summary: "Abort the app-started merge and return to the pre-merge HEAD.", expiresAt: Date.now() + 60000 };
    }
    if (action.startsWith("history_")) {
      if (targets.length !== 1) throw staleMockError("History actions confirm one commit.");
      mockConfirmTokens.set(token, `${action}:${targets[0]}`);
      return { confirmationToken: token, summary: `Demo confirmation for ${action} at ${targets[0].slice(0, 7)}.`, expiresAt: Date.now() + 60000 };
    }
    if (targets.length === 0) throw staleMockError("Select a branch.");
    const branches = mockBranches();
    const summaries: string[] = [];
    for (const refId of targets) {
      const target = branches.find((b) => b.refId === refId && b.kind === "local");
      if (!target) throw staleMockError("Unknown branch reference.");
      if (target.current) throw staleMockError("The current branch cannot be deleted.");
      const merged = !mockUnmergedBranches.has(target.refId);
      summaries.push(
        `Delete local branch '${target.label}' (${target.oid.slice(0, 12)}); ${merged ? "fully merged into HEAD" : "NOT merged into HEAD — safe delete will refuse"}`
      );
    }
    mockConfirmTokens.set(token, targets[0]);
    return { confirmationToken: token, summary: summaries.join("\n"), expiresAt: Date.now() + 60000 };
  },
  async branchDelete(_repoId: string, _expectedVersion: number, refId: string, confirmationToken: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const bound = mockConfirmTokens.get(confirmationToken);
    if (bound !== refId) throw staleMockError("This confirmation expired or no longer matches.");
    mockConfirmTokens.delete(confirmationToken);
    const branches = mockBranches();
    const index = branches.findIndex((b) => b.refId === refId && b.kind === "local");
    if (index < 0) throw staleMockError("Unknown branch reference.");
    if (branches[index].current) throw staleMockError("The current branch cannot be deleted.");
    if (mockUnmergedBranches.has(branches[index].refId)) {
      const error: AppError = { code: "REF_INVALID", message: "Branch is not merged; safe delete refuses.", recovery: "inspectState", retryable: false };
      throw error;
    }
    branches.splice(index, 1);
    return mockSessionWithCounts();
  },
  async historyPage(
    _repoId: string,
    _scope: HistoryScope,
    cursor: string | null,
    limit: number
  ): Promise<HistoryPage> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const rows = demoRows();
    const offset = cursor === null ? 0 : Number.parseInt(cursor, 10);
    const slice = rows.slice(offset, offset + limit);
    const next = offset + limit < rows.length ? String(offset + limit) : null;
    return { historySessionId: "demo-session", rows: slice, nextCursor: next, truncated: false };
  },
  async historySearch(
    _repoId: string,
    _scope: HistoryScope,
    query: string,
    cursor: string | null,
    limit: number
  ): Promise<SearchResults> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const needle = query.trim().toLowerCase();
    if (needle === "") return { rows: [], nextCursor: null, incomplete: false };
    // Literal substring match, mirroring the backend literal rule (T04).
    const matched = demoRows().filter(
      (row) =>
        row.subject.toLowerCase().includes(needle) ||
        row.authorName.toLowerCase().includes(needle) ||
        row.oid.toLowerCase().includes(needle)
    );
    const offset = cursor === null ? 0 : Number.parseInt(cursor, 10);
    const slice = matched.slice(offset, offset + limit);
    const next = offset + limit < matched.length ? String(offset + limit) : null;
    return { rows: slice, nextCursor: next, incomplete: false };
  },
  async repoStatus(repoId: string): Promise<StatusData> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    return { files: structuredClone(mockWorktree(repoId)) };
  },
  async indexStage(repoId: string, _expectedVersion: number, pathIds: string[]): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (pathIds.length === 0) throw staleMockError();
    for (const id of pathIds) {
      const row = mockWorktree(repoId).find((f) => f.pathId === id);
      if (!row) throw staleMockError();
      if (row.worktreeStatus !== " " && row.worktreeStatus !== "!") {
        row.indexStatus = row.worktreeStatus === "?" ? "A" : row.worktreeStatus;
        row.worktreeStatus = " ";
      }
    }
    return mockSessionWithCounts();
  },
  async indexUnstage(repoId: string, _expectedVersion: number, pathIds: string[]): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (pathIds.length === 0) throw staleMockError();
    for (const id of pathIds) {
      const row = mockWorktree(repoId).find((f) => f.pathId === id);
      if (!row) throw staleMockError();
      if (row.indexStatus !== " " && row.indexStatus !== "?" && row.indexStatus !== "!") {
        row.worktreeStatus = row.indexStatus === "A" ? "?" : row.indexStatus;
        row.indexStatus = " ";
      }
    }
    return mockSessionWithCounts();
  },
  async worktreeDiscardFile(repoId: string, _expectedVersion: number, pathId: string, confirmationToken: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (mockConfirmTokens.get(confirmationToken) !== `discard_file:${pathId}`) throw staleMockError("This confirmation expired or no longer matches.");
    mockConfirmTokens.delete(confirmationToken);
    const rows = mockWorktree(repoId);
    const index = rows.findIndex((file) => file.pathId === pathId);
    if (index < 0) throw staleMockError();
    const row = rows[index];
    if (row.worktreeStatus === "?") rows.splice(index, 1);
    else if (row.indexStatus === " ") rows.splice(index, 1);
    else row.worktreeStatus = " ";
    return mockSessionWithCounts();
  },
  async diffHunkStage(repoId: string, _expectedVersion: number, pathId: string, hunkId: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (hunkId !== "demo-hunk-1") throw staleMockError("The diff changed; refresh it and retry.");
    const row = mockWorktree(repoId).find((file) => file.pathId === pathId);
    if (!row || row.worktreeStatus === "?") throw staleMockError();
    row.indexStatus = "M";
    return mockSessionWithCounts();
  },
  async diffHunkDiscard(repoId: string, _expectedVersion: number, pathId: string, hunkId: string, confirmationToken: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (mockConfirmTokens.get(confirmationToken) !== `discard_hunk:${pathId}\0${hunkId}`) throw staleMockError("This confirmation expired or no longer matches.");
    mockConfirmTokens.delete(confirmationToken);
    const rows = mockWorktree(repoId);
    const index = rows.findIndex((file) => file.pathId === pathId);
    if (index < 0 || hunkId !== "demo-hunk-1") throw staleMockError();
    if (rows[index].indexStatus === " ") rows.splice(index, 1);
    else rows[index].worktreeStatus = " ";
    return mockSessionWithCounts();
  },
  async operationGet(operationId: string): Promise<OperationRecord> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      operationId,
      requestId: "demo-request",
      repoId: null,
      kind: "demo.noop",
      state: "succeeded",
      stage: "done",
      progress: null,
      errorCode: null,
      error: null
    };
  },
  async operationCancel(): Promise<{ cancelRequested: boolean }> {
    return { cancelRequested: false };
  },
  async remoteStatus(_repoId: string): Promise<RemoteStatus> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      remoteName: "origin",
      url: "https://example.com/demo/app.git",
      upstreamRef: "origin/main",
      ahead: 1,
      behind: 2,
      lastFetchAt: "2026-09-22T10:00:00Z"
    };
  },
  async bitbucketConnect(
    _repoId: string,
    _expectedVersion: number,
    _remote: string | null,
    _apiToken: string
  ): Promise<BitbucketConnectionResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      remoteName: "origin",
      username: "x-bitbucket-api-token-auth",
      credentialSaved: true
    };
  },
  async remoteFetch(_repoId: string, _expectedVersion: number, _remote: string | null): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: "demo-fetch-1" };
  },
  async remotePull(_repoId: string, _expectedVersion: number): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: "demo-pull-1" };
  },
  async remotePush(_repoId: string, _expectedVersion: number, _remote: string | null, _setUpstream: boolean): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: "demo-push-1" };
  },
  async stashList(_repoId: string): Promise<StashEntry[]> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return [
      { stashId: "stash@{0}", oid: "demo-stash-oid-1", label: "WIP on main: demo change", createdAt: 1758547200 },
      { stashId: "stash@{1}", oid: "demo-stash-oid-2", label: "WIP on main: older change", createdAt: 1758460800 }
    ];
  },
  async stashSave(_repoId: string, _expectedVersion: number, _message: string, _includeUntracked: boolean): Promise<StashSaveResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { oid: "demo-stash-oid-3", snapshot: structuredClone(demoSession), noChange: false };
  },
  async stashApply(_repoId: string, _expectedVersion: number, _stashId: string, _expectedOid: string, mode: "apply" | "pop"): Promise<StashApplyResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { snapshot: structuredClone(demoSession), retained: mode === "apply", conflicted: false, dropError: null };
  },
  async conflictList(_repoId: string): Promise<ConflictList> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    const files: ConflictFile[] = [
      {
        pathId: "demo:conflict:0",
        displayPath: "src/app/App.svelte",
        kind: "text",
        hasBase: true,
        hasCurrent: true,
        hasIncoming: true,
        supported: true,
        supportReason: null
      }
    ];
    return { files, canComplete: false, canAbort: true, abortReason: null };
  },
  async conflictPreview(_repoId: string, _pathId: string): Promise<ConflictPreview> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      pathId: "demo:conflict:0",
      displayPath: "src/app/App.svelte",
      base: { text: "base line\n", truncated: false },
      current: { text: "base line\nmain line\n", truncated: false },
      incoming: { text: "base line\nfeature line\n", truncated: false },
      workingFingerprint: "demofp:24",
      supportedActions: ["current", "incoming"],
      supportReason: null,
      currentLabel: "main",
      incomingLabel: "feature/ui"
    };
  },
  async conflictAccept(_repoId: string, _v: number, _p: string, _s: string, _f: string, _t: string): Promise<ConflictAcceptResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { snapshot: structuredClone(demoSession), workingFingerprint: "demofp:25" };
  },
  async conflictMarkResolved(_repoId: string, _v: number, _p: string, _f: string, _r: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async mergeStart(_repoId: string, _v: number, _s: string, _t: string): Promise<MergeStartResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    const snapshot = structuredClone(demoSession);
    snapshot.state = "merging";
    snapshot.mergeOrigin = "app";
    return { snapshot, conflicted: true, alreadyUpToDate: false };
  },
  async mergeComplete(_repoId: string, _v: number, _s: string, _b: string, _r: boolean, _h: string): Promise<MergeCompleteResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { oid: "demo-merge-oid", snapshot: structuredClone(demoSession) };
  },
  async mergeAbort(_repoId: string, _v: number, _t: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async branchMove(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async branchRename(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async branchSetUpstream(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async branchPush(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async workspacesSave(): Promise<WorkspacesSaved> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { saved: true };
  },
  async workspacesRestore(): Promise<WorkspacesRestoreResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { opened: [], skipped: [], activeKey: null };
  },
  async historyCheckout(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async tagCreateAt(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async historyPushTo(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async cherryPickOnto(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async revertCommit(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async mergeCommit(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async rebaseOnto(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async rewordMessage(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async modifyCommit(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async editAuthor(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async splitCommit(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async moveToBranch(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async rebaseInteractiveFrom(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async resetSoft(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async resetMixed(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async resetHard(): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return structuredClone(demoSession);
  },
  async settingsGet(): Promise<SettingsV1> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { ...mockSettings };
  },
  async settingsUpdate(settingsVersion: number, fontScale: number | null): Promise<SettingsV1> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    if (mockSettings.version !== settingsVersion) {
      throw { code: "STALE_STATE", message: "Settings changed under you; reload and retry.", recovery: "refresh", retryable: false };
    }
    if (fontScale === null || !Number.isFinite(fontScale) || fontScale < 0.875 || fontScale > 1.25) {
      throw { code: "INVALID_ARGUMENT", message: "Font scale must be between 0.875 and 1.25.", recovery: "inspectState", retryable: false };
    }
    mockSettings = { version: mockSettings.version + 1, fontScale };
    return { ...mockSettings };
  },
  async operationLog(_repoId: string, _cursor: number): Promise<OperationLogPage> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    const entries: OperationLogEntry[] = [
      { seq: 2, at: Date.now(), kind: "push", summary: "push origin (abc123)", outcome: "ok", errorCode: null },
      { seq: 1, at: Date.now(), kind: "fetch", summary: "fetch origin", outcome: "ok", errorCode: null }
    ];
    return { entries, nextCursor: 0 };
  },
  async commitDetails(_repoId: string, oid: string, parentIndex: number | null): Promise<CommitDetails> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const commit = demoCommits.find((c) => c.oid === oid) ?? demoCommits[0];
    return {
      oid: commit.oid,
      subject: commit.subject,
      body: "Demo body text.",
      authorName: commit.authorName,
      authoredAt: commit.committedAt,
      committedAt: commit.committedAt,
      parents: [...commit.parents],
      parentIndex: commit.parents.length === 0 ? null : (parentIndex ?? 0),
      files: [{ status: "M", path: "src/app/App.svelte", oldPath: null, pathId: "demo:c1:0" }]
    };
  },
  async diffRead(_repoId: string, target: DiffTarget): Promise<DiffDocument> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    const displayPath =
      target.kind === "commit" ? `commit:${target.oid.slice(0, 8)}` : `worktree:${target.pathId}`;
    return {
      kind: "text",
      displayPath,
      additions: 2,
      deletions: 1,
      hunks: [
        {
          hunkId: "demo-hunk-1",
          header: "@@ -12,7 +12,9 @@",
          oldStart: 12,
          newStart: 12,
          lines: [
            { kind: "context", oldLine: 12, newLine: 12, text: "rows.map((row) => paint(row));" },
            { kind: "delete", oldLine: 13, newLine: null, text: "lane = nextFreeLane();" },
            { kind: "add", oldLine: null, newLine: 13, text: "// keep lane across page boundary" },
            { kind: "add", oldLine: null, newLine: 14, text: "lane = carryLane(row);" },
            { kind: "noNewline", oldLine: null, newLine: null, text: "" }
          ]
        }
      ],
      truncated: false
    };
  }
};

/** In-memory demo worktree: stage/unstage mutate it, repoStatus reads it. */
const mockWorktreeByRepo = new Map<string, ChangedFile[]>();

function seedMockWorktree(repoId: string): ChangedFile[] {
  return [
    { pathId: `${repoId}:1:0`, displayPath: "src/app/App.svelte", indexStatus: "M", worktreeStatus: "M", kind: "unknown", conflicted: false },
    { pathId: `${repoId}:1:1`, displayPath: "src/lib/ipc/types.ts", indexStatus: "M", worktreeStatus: " ", kind: "unknown", conflicted: false },
    { pathId: `${repoId}:1:2`, displayPath: "new notes.txt", indexStatus: " ", worktreeStatus: "?", kind: "unknown", conflicted: false }
  ];
}

function mockWorktree(repoId: string): ChangedFile[] {
  let rows = mockWorktreeByRepo.get(repoId);
  if (!rows) {
    rows = seedMockWorktree(repoId);
    mockWorktreeByRepo.set(repoId, rows);
  }
  return rows;
}

/** Test hook: restore the demo worktree to its seeded rows. */
function resetMockWorktree(repoId?: string): void {
  if (repoId === undefined) mockWorktreeByRepo.clear();
  else mockWorktreeByRepo.delete(repoId);
}

let mockBranchStore: RefItem[] | null = null;
let mockCommitCounter = 0;
let mockConfirmCounter = 0;
const mockConfirmTokens = new Map<string, string>();
/** Branches known to be unmerged (safe delete refuses them). */
const mockUnmergedBranches = new Set<string>(["refs/heads/feature/ui"]);

function mockBranches(): RefItem[] {
  if (!mockBranchStore) {
    mockBranchStore = [
      { refId: "refs/remotes/origin/main", fullName: "refs/remotes/origin/main", label: "origin/main", kind: "remote", oid: demoCommits[0].oid, current: false, checkedOutElsewhere: false },
      { refId: "refs/heads/main", fullName: "refs/heads/main", label: "main", kind: "local", oid: demoCommits[0].oid, current: true, checkedOutElsewhere: false },
      { refId: "refs/heads/feature/ui", fullName: "refs/heads/feature/ui", label: "feature/ui", kind: "local", oid: demoCommits[1].oid, current: false, checkedOutElsewhere: false }
    ];
  }
  return mockBranchStore;
}

/** Test hook: restore the demo branch list. */
function resetMockBranches(): void {
  mockBranchStore = null;
  mockConfirmTokens.clear();
  mockUnmergedBranches.clear();
  mockUnmergedBranches.add("refs/heads/feature/ui");
}

function staleMockError(message = "Demo file list is outdated; refresh and retry."): AppError {
  return { code: "STALE_STATE", message, recovery: "refresh", retryable: false };
}

function mockSessionWithCounts(): RepoSnapshot {
  const session = structuredClone(demoSession);
  let staged = 0;
  let unstaged = 0;
  for (const rows of mockWorktreeByRepo.values()) {
    for (const f of rows) {
      if (![" ", "?", "!"].includes(f.indexStatus)) staged += 1;
      if (![" ", "!"].includes(f.worktreeStatus)) unstaged += 1;
    }
  }
  const current = mockBranches().find(branch => branch.current);
  if (current && session.head.kind === "branch") session.head = { kind: "branch", refId: current.refId, name: current.label, oid: current.oid };
  session.trust = "trusted";
  session.stagedCount = staged;
  session.unstagedCount = unstaged;
  session.conflictCount = 0;
  return session;
}

return { ...mockAdapter, resetMockWorktree, resetMockBranches };
}

export const mockAdapter = createMockAdapter();
export const resetMockWorktree = mockAdapter.resetMockWorktree;
export const resetMockBranches = mockAdapter.resetMockBranches;
