import { editableMergeText } from "../conflict/merge";
import { aliasValidationError } from "../repositories/alias";
import { demoPreflight } from "../../mocks/demoPreflight";
import { demoCommits } from "../../mocks/demoRepo";
import { demoRecents, demoSession as initialDemoSession } from "../../mocks/demoSession";
import type {
  AppError,
  AppPreflightRequest,
  BitbucketConnectionResult,
  BranchCompareResult,
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
  ConflictHunks,
  ConflictAutoResolveResult,
  ConflictList,
  ConflictMergeResult,
  ConflictPreview,
  ResolvedConflictFile,
  MergeBlockPick,
  MergeSegment,
  MergeCompleteResult,
  MergeStartResult,
  SettingsV1,
  OperationLogEntry,
  OperationLogPage,
  OperationRecord,
  OperationStarted,
  PullRequestResult,
  PreflightData,
  RemoteStatus,
  StashApplyResult,
  StashEntry,
  StashSaveResult,
  RecentEntry,
  RepositoryAlias,
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

let mockSettings: SettingsV1 = { version: 1, fontScale: 1, autoFetchMinutes: 5 };
const demoAliases = new Map<string, string>();
const DEMO_ALIAS_PREFIX = "octopus.demo.alias:";

/**
 * Explicit browser demo / test adapter. Shares the same DTOs as the real
 * client but never touches native APIs. The UI must label it "Demo data".
 */
export function createMockAdapter(seed: RepoSnapshot = initialDemoSession) {
const demoSession = structuredClone(seed);
let conflictDoc: ConflictHunks | null = null;
let syncCounter = 0;
let lastFetchAt = "2026-09-22T10:00:00Z";
let commitRows = demoRows();
const commitBodies = new Map<string, string>();
const supersededCommits = new Map<string, CommitRow>();
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
  async repoClone(sourceUrl: string, _selectedPath: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 50));
    if (sourceUrl.trim() === "") {
      throw { code: "INVALID_ARGUMENT", message: "Remote URL is empty", recovery: "inspectState", retryable: false } satisfies AppError;
    }
    mockWorktree(demoSession.repoId);
    return mockSessionWithCounts();
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
  async repoAliasGet(_repoId: string): Promise<RepositoryAlias> {
    const key = demoSession.workspaceKey;
    return { workspaceKey: key, alias: typeof localStorage === "undefined" ? demoAliases.get(key) ?? null : localStorage.getItem(DEMO_ALIAS_PREFIX + key) };
  },
  async repoAliasSet(_repoId: string, alias: string | null): Promise<RepositoryAlias> {
    const error = aliasValidationError(alias ?? "");
    if (error) throw { code: "INVALID_ARGUMENT", message: error, recovery: "inspectState", retryable: false } satisfies AppError;
    const next = alias?.trim() || null;
    const key = demoSession.workspaceKey;
    if (typeof localStorage !== "undefined") {
      if (next === null) localStorage.removeItem(DEMO_ALIAS_PREFIX + key);
      else localStorage.setItem(DEMO_ALIAS_PREFIX + key, next);
    }
    if (next === null) demoAliases.delete(key);
    else demoAliases.set(key, next);
    return { workspaceKey: key, alias: next };
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
  async commitCreate(repoId: string, expectedVersion: number, subject: string, body: string, amendOid: string | null = null): Promise<CommitResult> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (subject.trim() === "") {
      const error: AppError = { code: "INVALID_ARGUMENT", message: "Commit subject cannot be empty.", recovery: "inspectState", retryable: false };
      throw error;
    }
    const rows = mockWorktree(repoId);
    const head = mockSessionWithCounts().head;
    if (amendOid !== null) {
      if (head.kind === "unborn") throw { code: "INVALID_ARGUMENT", message: "There is no commit to amend yet.", recovery: "inspectState", retryable: false } satisfies AppError;
      if (head.oid !== amendOid || expectedVersion !== demoSession.version) throw staleMockError("HEAD changed; refresh and select Amend again.");
      if (demoSession.state !== "normal" || demoSession.mergeOrigin || rows.some(f => f.conflicted)) {
        throw { code: "CONFLICTS_PRESENT", message: "Finish the current Git operation before amending.", recovery: "inspectState", retryable: false } satisfies AppError;
      }
    }
    if (amendOid === null && !rows.some((f) => ![" ", "?", "!"].includes(f.indexStatus))) {
      const error: AppError = { code: "EMPTY_INDEX", message: "Nothing is staged.", recovery: "inspectState", retryable: false };
      throw error;
    }
    mockCommitCounter += 1;
    const oid = mockCommitCounter.toString(16).padStart(40, "a");
    const previous = commitRows.find(row => row.oid === amendOid);
    const currentRef = head.kind === "branch" ? head.refId : head.kind === "unborn" ? `refs/heads/${head.name}` : null;
    const row: CommitRow = {
      oid, subject: subject.trim(), parents: previous?.parents ?? (head.kind === "unborn" ? [] : [head.oid]),
      authorName: previous?.authorName ?? "Demo Author", authoredAt: previous?.authoredAt ?? new Date().toISOString(),
      committedAt: new Date().toISOString(), refs: currentRef ? [currentRef] : [], boundary: false
    };
    // All-refs history retains the old commit only while another ref or
    // child still reaches it. Its details remain readable by object ID.
    if (previous) supersededCommits.set(previous.oid, previous);
    const remaining = commitRows.map(entry => ({ ...entry, refs: entry.refs.filter(ref => ref !== currentRef) }));
    commitRows = [row, ...remaining.filter(entry => entry.oid !== amendOid || entry.refs.length > 0 || remaining.some(child => child.parents.includes(entry.oid)))];
    commitBodies.set(oid, body.trim());
    if (head.kind === "detached") demoSession.head = { kind: "detached", oid };
    else {
      const name = head.name;
      demoSession.head = { kind: "branch", refId: currentRef!, name, oid };
      const branch = mockBranches().find(ref => ref.refId === currentRef);
      if (branch) branch.oid = oid;
    }
    demoSession.version += 1;
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
  async branchCompare(_repoId: string, localRefId: string, remoteRefId: string): Promise<BranchCompareResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    const branches = mockBranches();
    const local = branches.find((b) => b.refId === localRefId);
    const remote = branches.find((b) => b.refId === remoteRefId);
    if (!local || !remote) throw staleMockError("Unknown branch reference.");
    if (local.kind !== "local" || remote.kind !== "remote") {
      throw { code: "INVALID_ARGUMENT", message: "Compare target must be a local branch and base a remote branch.", recovery: "inspectState", retryable: false } satisfies AppError;
    }
    if (local.oid === remote.oid) return { ahead: 0, behind: 0 };
    if (localRefId === "refs/heads/feature/diverged" && remoteRefId === "refs/remotes/origin/feature/diverged") {
      return { ahead: 2, behind: 1 };
    }
    return { ahead: 1, behind: 0 };
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
      return { confirmationToken: token, summary: `Discard all unstaged changes in '${file.displayPath}'. This cannot be undone by Octopus.`, expiresAt: Date.now() + 60000 };
    }
    if (action === "discard_hunk") {
      if (targets.length !== 2) throw staleMockError("Discard hunk confirms one hunk.");
      const file = mockWorktree(repoId).find((row) => row.pathId === targets[0]);
      if (!file) throw staleMockError("The file list changed.");
      mockConfirmTokens.set(token, `discard_hunk:${targets.join("\0")}`);
      return { confirmationToken: token, summary: `Discard this hunk from '${file.displayPath}' and restore its lines from the index. This cannot be undone by Octopus.`, expiresAt: Date.now() + 60000 };
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
    const rows = commitRows;
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
    const matched = commitRows.filter(
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
    const files = structuredClone(mockWorktree(repoId));
    if ((demoSession.conflictCount ?? 0) > 0) files.push({ pathId: conflictDoc?.pathId ?? "demo:conflict:0", displayPath: "src/app/App.svelte", indexStatus: "U", worktreeStatus: "U", kind: "text", conflicted: true });
    return { files };
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
  async diffLinesStage(repoId: string, _expectedVersion: number, pathId: string, hunkId: string, lines: number[]): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (hunkId !== "demo-hunk-1" && hunkId !== "demo-hunk-2") throw staleMockError("The diff changed; refresh it and retry.");
    if (!Array.isArray(lines) || lines.length === 0) throw staleMockError("Select at least one changed line.");
    const row = mockWorktree(repoId).find((file) => file.pathId === pathId);
    if (!row || row.worktreeStatus === "?") throw staleMockError();
    row.indexStatus = "M";
    return mockSessionWithCounts();
  },
  async diffLinesUnstage(repoId: string, _expectedVersion: number, pathId: string, hunkId: string, lines: number[]): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    if (hunkId !== "demo-hunk-1" && hunkId !== "demo-hunk-2") throw staleMockError("The diff changed; refresh it and retry.");
    if (!Array.isArray(lines) || lines.length === 0) throw staleMockError("Select at least one changed line.");
    const row = mockWorktree(repoId).find((file) => file.pathId === pathId);
    if (!row) throw staleMockError();
    row.worktreeStatus = "M";
    return mockSessionWithCounts();
  },
  async operationGet(operationId: string): Promise<OperationRecord> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      operationId,
      requestId: "demo-request",
      repoId: null,
      kind: operationId.startsWith("demo-fetch-") ? "fetch" : operationId.startsWith("demo-pull-") ? "pull" : operationId.startsWith("demo-push-") ? "push" : "demo.noop",
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
      lastFetchAt
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
  async pullRequestCreate(
    _repoId: string,
    _expectedVersion: number,
    _remote: string | null,
    _sourceBranch: string,
    _targetBranch: string,
    _title: string,
    _description: string
  ): Promise<PullRequestResult> {
    await new Promise((resolve) => setTimeout(resolve, 30));
    return {
      provider: "github",
      url: "https://example.com/demo/app/pull/7",
      reference: "#7"
    };
  },
  async remoteFetch(_repoId: string, _expectedVersion: number, _remote: string | null): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    lastFetchAt = new Date().toISOString();
    return { operationId: `demo-fetch-${++syncCounter}` };
  },
  async remotePull(_repoId: string, _expectedVersion: number): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: `demo-pull-${++syncCounter}` };
  },
  async remotePush(_repoId: string, _expectedVersion: number, _remote: string | null, _setUpstream: boolean): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: `demo-push-${++syncCounter}` };
  },
  async remotePushForce(_repoId: string, _expectedVersion: number, _remote: string | null, _setUpstream: boolean): Promise<OperationStarted> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { operationId: "demo-push-force-1" };
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
        pathId: conflictDoc?.pathId ?? "demo:conflict:0",
        displayPath: "src/app/App.svelte",
        kind: "text",
        hasBase: true,
        hasCurrent: true,
        hasIncoming: true,
        supported: true,
        supportReason: null
      }
    ];
    const resolvedFiles: ResolvedConflictFile[] = demoSession.state === "merging"
      ? [
          { displayPath: "README.md", originalPath: null, status: "M" },
          { displayPath: "src/lib/conflict/merge.ts", originalPath: null, status: "M" },
          { displayPath: "src/lib/new-helper.ts", originalPath: null, status: "A" },
          { displayPath: "src/lib/legacy-helper.ts", originalPath: null, status: "D" },
          ...((demoSession.conflictCount ?? 0) === 0
            ? [{ displayPath: "src/app/App.svelte", originalPath: null, status: "M" }]
            : [])
        ]
      : [];
    return {
      files: (demoSession.conflictCount ?? 0) > 0 ? files : [],
      resolvedFiles,
      currentLabel: "main",
      incomingLabel: "feature/ui",
      canComplete: demoSession.state === "merging" && demoSession.conflictCount === 0,
      canAbort: demoSession.state === "merging",
      abortReason: null
    };
  },
  async conflictPreview(_repoId: string, _pathId: string): Promise<ConflictPreview> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return {
      pathId: conflictDoc?.pathId ?? "demo:conflict:0",
      displayPath: "src/app/App.svelte",
      base: { text: "base line\n", truncated: false },
      current: { text: "base line\nmain line\n", truncated: false },
      incoming: { text: "base line\nfeature line\n", truncated: false },
      workingFingerprint: conflictDoc?.workingFingerprint ?? "demofp:24",
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
    demoSession.conflictCount = 0;
    return structuredClone(demoSession);
  },
  async conflictMarkAllResolved(_repoId: string, _v: number): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    demoSession.conflictCount = 0;
    return structuredClone(demoSession);
  },
  async conflictHunks(_repoId: string, _pathId: string): Promise<ConflictHunks> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    if (conflictDoc) {
      if (_pathId !== conflictDoc.pathId) throw staleMockError();
      return structuredClone(conflictDoc);
    }
    const doc: ConflictHunks = {
      pathId: "demo:conflict:0",
      displayPath: "src/app/App.svelte",
      currentLabel: "main",
      incomingLabel: "feature/ui",
      workingFingerprint: "demofp:24",
      workingText: "",
      segments: [
        { kind: "clean", lines: ["<script>", "  let count = 0;"] },
        {
          kind: "conflict",
          hunkId: "demo-hunk-1",
          current: ["  let label = \"main\";", "  label += \"!\";"],
          incoming: ["  let label = \"feature\";"],
          base: ["  let label = \"base\";"],
          raw: "<<<<<<< HEAD\n  let label = \"main\";\n  label += \"!\";\n=======\n  let label = \"feature\";\n>>>>>>> feature/ui\n"
        },
        { kind: "clean", lines: ["  $effect(() => paint(count));", "</script>"] },
        {
          kind: "conflict",
          hunkId: "demo-hunk-2",
          current: ["  const cap = 24;", "  const floor = 0;"],
          incoming: ["  const cap = 8;", "  const floor = 1;"],
          base: ["  const cap = 8;", "  const floor = 0;"],
          raw: "<<<<<<< HEAD\n  const cap = 24;\n  const floor = 0;\n=======\n  const cap = 8;\n  const floor = 1;\n>>>>>>> feature/ui\n"
        }
      ],
      conflictCount: 2
    };
    doc.workingText = doc.segments.map((segment) => segment.kind === "clean" ? segment.lines.join("\n") + "\n" : segment.raw).join("");
    conflictDoc = doc;
    return structuredClone(doc);
  },
  async conflictAutoResolve(repoId: string, _version: number, pathId: string, fingerprint: string): Promise<ConflictAutoResolveResult> {
    const doc = await mockAdapter.conflictHunks(repoId, pathId);
    if (doc.workingFingerprint !== fingerprint) throw staleMockError();
    const picks: MergeBlockPick[] = [];
    // The demo fixture has two equal-length, unambiguous source ranges. Native
    // resolution uses the Rust engine and the live index, including real bases.
    for (const block of doc.segments) {
      if (block.kind !== "conflict") continue;
      const same = (a: string[], b: string[]) => a.length === b.length && a.every((line, i) => line === b[i]);
      if (same(block.current, block.incoming)) {
        picks.push({ hunkId: block.hunkId, lines: block.current.map((_, index) => ({ side: "current", index })) });
      } else if (block.base.length && block.current.length === block.base.length && block.incoming.length === block.base.length) {
        const lines: MergeBlockPick["lines"] = [];
        let ambiguous = false;
        for (let index = 0; index < block.base.length; index++) {
          const current = block.current[index]; const incoming = block.incoming[index]; const base = block.base[index];
          if (current === incoming || incoming === base) lines.push({ side: "current", index });
          else if (current === base) lines.push({ side: "incoming", index });
          else { ambiguous = true; break; }
        }
        if (!ambiguous) picks.push({ hunkId: block.hunkId, lines });
      }
    }
    return { workingFingerprint: fingerprint, picks };
  },
  async conflictMerge(
    _repoId: string, _v: number, _p: string, fingerprint: string,
    picks: MergeBlockPick[], resultText: string | null = null
  ): Promise<ConflictMergeResult> {
    const doc = await mockAdapter.conflictHunks(_repoId, _p);
    if (fingerprint !== doc.workingFingerprint) throw staleMockError();
    const workingText = resultText ?? editableMergeText(doc, Object.fromEntries(picks.map((pick) => [pick.hunkId, pick.lines])));
    const segments: MergeSegment[] = [];
    // Demo-only marker parser, preserving enough state to reopen partial edits.
    const pattern = /^<<<<<<<[^\n]*\n([\s\S]*?)^=======\r?\n([\s\S]*?)^>>>>>>>[^\n]*(?:\n|$)/gm;
    let offset = 0;
    const lines = (text: string) => text.replace(/\r\n/g, "\n").replace(/\n$/, "").split("\n");
    for (const match of workingText.matchAll(pattern)) {
      if (match.index > offset) segments.push({ kind: "clean", lines: lines(workingText.slice(offset, match.index)) });
      segments.push({ kind: "conflict", hunkId: `demo-edited-${match.index}`, current: match[1] ? lines(match[1]) : [], incoming: match[2] ? lines(match[2]) : [], base: [], raw: match[0] });
      offset = match.index + match[0].length;
    }
    if (offset < workingText.length) segments.push({ kind: "clean", lines: lines(workingText.slice(offset)) });
    const remainingBlocks = segments.filter((segment) => segment.kind === "conflict").length;
    conflictDoc = { ...doc, pathId: `${doc.pathId}:saved`, segments, workingText, conflictCount: remainingBlocks, workingFingerprint: `${fingerprint}-saved` };
    return { snapshot: structuredClone(demoSession), workingFingerprint: conflictDoc.workingFingerprint,
      resolvedBlocks: Math.max(0, doc.conflictCount - remainingBlocks), remainingBlocks };
  },
  async mergeStart(_repoId: string, _v: number, source: string, _t: string): Promise<MergeStartResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    conflictDoc = null;
    demoSession.state = "merging";
    demoSession.mergeOrigin = "app";
    // The origin twin merges cleanly so the one-click auto-complete path
    // is exercisable; every other source conflicts as before.
    const clean = source === "refs/remotes/origin/main";
    demoSession.conflictCount = clean ? 0 : 1;
    return { snapshot: structuredClone(demoSession), conflicted: !clean, alreadyUpToDate: false };
  },
  async mergeComplete(_repoId: string, _v: number, _s: string, _b: string, _r: boolean, _h: string): Promise<MergeCompleteResult> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    demoSession.state = "normal";
    demoSession.mergeOrigin = null;
    demoSession.conflictCount = 0;
    return { oid: "demo-merge-oid", snapshot: structuredClone(demoSession) };
  },
  async mergeAbort(_repoId: string, _v: number, _t: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    demoSession.state = "normal";
    demoSession.mergeOrigin = null;
    demoSession.conflictCount = 0;
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
  async resetHard(_repoId: string, _expectedVersion: number, oid: string, _confirmationToken: string): Promise<RepoSnapshot> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    const current = mockBranches().find((b) => b.kind === "local" && b.current);
    if (current) current.oid = oid;
    return mockSessionWithCounts();
  },
  async settingsGet(): Promise<SettingsV1> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    return { ...mockSettings };
  },
  async settingsUpdate(settingsVersion: number, fontScale: number | null, autoFetchMinutes: number | null = null): Promise<SettingsV1> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    if (mockSettings.version !== settingsVersion) {
      throw { code: "STALE_STATE", message: "Settings changed under you; reload and retry.", recovery: "refresh", retryable: false };
    }
    if (fontScale === null && autoFetchMinutes === null) {
      throw { code: "INVALID_ARGUMENT", message: "Set at least one settings field.", recovery: "inspectState", retryable: false };
    }
    if (fontScale !== null && (!Number.isFinite(fontScale) || fontScale < 0.875 || fontScale > 1.25)) {
      throw { code: "INVALID_ARGUMENT", message: "Font scale must be between 0.875 and 1.25.", recovery: "inspectState", retryable: false };
    }
    if (autoFetchMinutes !== null && ![0, 1, 5, 10, 15].includes(autoFetchMinutes)) {
      throw { code: "INVALID_ARGUMENT", message: "Auto fetch must be off, or every 1, 5, 10 or 15 minutes.", recovery: "inspectState", retryable: false };
    }
    mockSettings = { version: mockSettings.version + 1, fontScale: fontScale ?? mockSettings.fontScale,
      autoFetchMinutes: autoFetchMinutes ?? mockSettings.autoFetchMinutes };
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
    const commit = commitRows.find((c) => c.oid === oid) ?? supersededCommits.get(oid) ?? commitRows[0];
    return {
      oid: commit.oid,
      subject: commit.subject,
      body: commitBodies.get(oid) ?? "Demo body text.",
      authorName: commit.authorName,
      authoredAt: commit.authoredAt,
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
        },
        {
          hunkId: "demo-hunk-2",
          header: "@@ -30,4 +30,5 @@",
          oldStart: 30,
          newStart: 30,
          lines: [
            { kind: "context", oldLine: 30, newLine: 30, text: "flush(viewport);" },
            { kind: "delete", oldLine: 31, newLine: null, text: "limit = 24;" },
            { kind: "add", oldLine: null, newLine: 31, text: "limit = laneCount;" },
            { kind: "add", oldLine: null, newLine: 32, text: "clamp(limit, 1, 64);" },
            { kind: "context", oldLine: 32, newLine: 33, text: "paint(lanes);" }
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
      { refId: "refs/heads/feature/ui", fullName: "refs/heads/feature/ui", label: "feature/ui", kind: "local", oid: demoCommits[1].oid, current: false, checkedOutElsewhere: false },
      { refId: "refs/heads/feature/diverged", fullName: "refs/heads/feature/diverged", label: "feature/diverged", kind: "local", oid: demoCommits[1].oid, current: false, checkedOutElsewhere: false },
      { refId: "refs/remotes/origin/feature/diverged", fullName: "refs/remotes/origin/feature/diverged", label: "origin/feature/diverged", kind: "remote", oid: demoCommits[2].oid, current: false, checkedOutElsewhere: false }
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
  session.conflictCount = demoSession.conflictCount ?? 0;
  return session;
}

return { ...mockAdapter, resetMockWorktree, resetMockBranches };
}

export const mockAdapter = createMockAdapter();
export const resetMockWorktree = mockAdapter.resetMockWorktree;
export const resetMockBranches = mockAdapter.resetMockBranches;
