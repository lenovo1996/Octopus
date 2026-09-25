// IPC v1 contracts — keep aligned with Rust domain types and command DTOs.
// Rust DTOs (src-tauri/src/domain) are canonical; this file must stay in parity
// (field names camelCase, same union variants). Covered by tests/unit/ipc.test.ts.

export type RepoId = string;
export type RequestId = string;
export type OperationId = string;
export type Oid = string;
export type Version = number;
export type PathId = string;
export type RefId = string;

export type ApiResult<T> =
  | { ok: true; data: T; requestId: RequestId }
  | { ok: false; error: AppError; requestId: RequestId };

export type ErrorCode =
  | "GIT_NOT_FOUND"
  | "GIT_VERSION_UNSUPPORTED"
  | "NOT_REPOSITORY"
  | "BARE_REPOSITORY"
  | "REPO_UNAVAILABLE"
  | "INVALID_ARGUMENT"
  | "PATH_INVALID"
  | "REF_INVALID"
  | "TRUST_REQUIRED"
  | "REPO_BUSY"
  | "REPO_LOCKED"
  | "STALE_STATE"
  | "DIRTY_WORKTREE"
  | "EMPTY_INDEX"
  | "IDENTITY_MISSING"
  | "CONFLICTS_PRESENT"
  | "DIVERGED"
  | "AUTH_REQUIRED"
  | "NETWORK_ERROR"
  | "OFFLINE"
  | "HOOK_FAILED"
  | "SIGNING_FAILED"
  | "OUTPUT_LIMIT"
  | "UNSUPPORTED"
  | "CANCELLED"
  | "TIMEOUT"
  | "IO_ERROR"
  | "GIT_ERROR";

export type RecoveryAction =
  | "refresh"
  | "retryRead"
  | "configureGit"
  | "authenticate"
  | "resolveConflict"
  | "chooseRepository"
  | "inspectState"
  | "none";

export interface AppError {
  code: ErrorCode;
  /** Plain text, user-safe. Never contains secrets or raw stderr. */
  message: string;
  recovery: RecoveryAction;
  /** Mutations must never auto-retry; reads may offer manual retry. */
  retryable: boolean;
  operationId?: OperationId;
  /** Local redacted-log lookup key, never rendered verbatim. */
  diagnosticId?: string;
}

export interface PreflightFeatures {
  /** System Git available for local (non-network) operations. */
  localOperations: boolean;
}

export interface PreflightData {
  /** Resolved executable path used for `git --version`. Display only. */
  gitPath: string;
  /** Raw version string reported by Git, e.g. "2.43.0". */
  gitVersion: string;
  /** True when the version meets the supported baseline (>= 2.43). */
  gitSupported: boolean;
  platform: string;
  arch: string;
  features: PreflightFeatures;
}

export interface AppPreflightRequest {
  requestId: RequestId;
}

export type HeadState =
  | { kind: "branch"; refId: string; name: string; oid: string }
  | { kind: "detached"; oid: string }
  | { kind: "unborn"; name: string };

export type TrustState = "readOnly" | "trusted";

export interface RepoSnapshot {
  repoId: RepoId;
  /** Stable exact worktree identity, only for local presentation persistence. */
  workspaceKey: string;
  version: Version;
  displayName: string;
  /** Display only; never round-tripped back into a mutation. */
  displayPath: string;
  head: HeadState;
  trust: TrustState;
  state: "normal" | "merging" | "stashConflict" | "externalOperation";
  mergeOrigin: "app" | "external" | null;
  upstream: { refId: string; ahead: number; behind: number } | null;
  lastFetchAt: string | null;
  activeOperation: OperationId | null;
  stagedCount: number | null;
  unstagedCount: number | null;
  conflictCount: number | null;
}

export interface RecentEntry {
  entryId: string;
  key: string;
  displayPath: string;
  lastOpenedAt: number;
}

export interface RepoOpenRequest {
  requestId: RequestId;
  selectedPath: string;
}

export interface RepoInitRequest {
  requestId: RequestId;
  selectedPath: string;
  initialBranch: string;
}

export interface RepoSnapshotRequest {
  requestId: RequestId;
  repoId: RepoId;
  refresh: boolean;
}

export interface RepoTrustRequest {
  requestId: RequestId;
  repoId: RepoId;
  trusted: boolean;
}

export interface RefItem {
  refId: string;
  fullName: string;
  label: string;
  kind: "local" | "remote" | "tag";
  /** Peeled commit oid. */
  oid: string;
  current: boolean;
  checkedOutElsewhere: boolean;
}

export interface CommitRow {
  oid: string;
  parents: string[];
  subject: string;
  authorName: string;
  authoredAt: string;
  committedAt: string;
  refs: string[];
  boundary: boolean;
}

export interface HistoryPage {
  historySessionId: string;
  rows: CommitRow[];
  nextCursor: string | null;
  truncated: boolean;
}

export interface HistoryScope {
  type: "allRefs" | "head" | "ref";
  refId?: string;
}

export interface CommitFileChange {
  status: string;
  path: string;
  oldPath: string | null;
  /** Opaque token into this commit + parent file list (T08). Empty until loaded. */
  pathId: string;
}

export type DiffTarget =
  | { kind: "worktree"; pathId: string }
  | { kind: "index"; pathId: string }
  | { kind: "commit"; oid: string; parentIndex: number | null; pathId: string };

export interface DiffLine {
  kind: "context" | "add" | "delete" | "noNewline";
  oldLine: number | null;
  newLine: number | null;
  text: string;
}

export interface DiffHunk {
  hunkId: string;
  header: string;
  oldStart: number;
  newStart: number;
  lines: DiffLine[];
}

export interface DiffDocument {
  kind: "text" | "binary" | "symlink" | "submodule" | "tooLarge";
  displayPath: string;
  additions: number | null;
  deletions: number | null;
  hunks: DiffHunk[];
  truncated: boolean;
  reason?: string;
}

export interface CommitDetails {
  oid: string;
  subject: string;
  body: string;
  authorName: string;
  authoredAt: string;
  committedAt: string;
  parents: string[];
  parentIndex: number | null;
  files: CommitFileChange[];
}

export interface SearchResults {
  rows: CommitRow[];
  nextCursor: string | null;
  incomplete: boolean;
}

export interface ChangedFile {
  pathId: string;
  displayPath: string;
  originalDisplayPath?: string;
  indexStatus: string;
  worktreeStatus: string;
  kind: string;
  conflicted: boolean;
}

export interface StatusData {
  files: ChangedFile[];
}

export type OperationState =
  | "queued"
  | "running"
  | "cancelling"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "unknown";

export interface OperationRecord {
  operationId: string;
  requestId: string;
  repoId: string | null;
  kind: string;
  state: OperationState;
  stage: string;
  progress: number | null;
  errorCode: string | null;
  /** Structured, user-safe terminal failure; optional for older native builds. */
  error?: AppError | null;
}

export interface RepoInvalidatedEvent {
  repoId: string;
  version: number;
  reason: string;
}

export interface RemoteStatus {
  remoteName: string | null;
  url: string | null;
  upstreamRef: string | null;
  ahead: number | null;
  behind: number | null;
  lastFetchAt: string | null;
}

export interface BitbucketConnectionResult {
  remoteName: string;
  username: string;
  credentialSaved: boolean;
}

export interface PullRequestResult {
  /** "bitbucket" | "github" | "gitlab". */
  provider: string;
  url: string;
  /** "#7" (PR) or "!3" (GitLab MR). */
  reference: string;
}

export interface OperationStarted {
  operationId: string;
}

export interface OperationLogEntry {
  seq: number;
  at: number;
  kind: string;
  summary: string;
  outcome: "ok" | "error" | "cancelled";
  errorCode: string | null;
}

export interface OperationLogPage {
  entries: OperationLogEntry[];
  nextCursor: number;
}

export interface StashEntry {
  stashId: string;
  oid: string;
  label: string;
  createdAt: number;
}

export interface StashSaveResult {
  oid: string | null;
  snapshot: RepoSnapshot;
  noChange: boolean;
}

export interface StashApplyResult {
  snapshot: RepoSnapshot;
  retained: boolean;
  conflicted: boolean;
  dropError: AppError | null;
}

export interface ConflictFile {
  pathId: string;
  displayPath: string;
  kind: string;
  hasBase: boolean;
  hasCurrent: boolean;
  hasIncoming: boolean;
  supported: boolean;
  supportReason: string | null;
}

export interface ConflictList {
  files: ConflictFile[];
  canComplete: boolean;
  canAbort: boolean;
  abortReason: string | null;
}

export interface StagePreview {
  text: string;
  truncated: boolean;
}

export interface ConflictPreview {
  pathId: string;
  displayPath: string;
  base: StagePreview | null;
  current: StagePreview | null;
  incoming: StagePreview | null;
  workingFingerprint: string;
  supportedActions: string[];
  supportReason: string | null;
  currentLabel: string;
  incomingLabel: string;
}

export interface MergeStartResult {
  snapshot: RepoSnapshot;
  conflicted: boolean;
  alreadyUpToDate: boolean;
}

export interface SettingsV1 {
  version: number;
  fontScale: number;
}

export interface MergeCompleteResult {
  oid: string;
  snapshot: RepoSnapshot;
}

export interface ConflictAcceptResult {
  snapshot: RepoSnapshot;
  workingFingerprint: string;
}

export interface IdentityInfo {
  name: string | null;
  email: string | null;
  scope: string;
  signing: boolean;
}

export interface CommitResult {
  oid: string;
  snapshot: RepoSnapshot;
}

export interface BranchCreateResult {
  snapshot: RepoSnapshot;
  switched: boolean;
  switchError?: AppError;
}

export interface ConfirmationDetails {
  confirmationToken: string;
  summary: string;
  expiresAt: number;
}

export interface OpenWorkspaceEntry {
  key: string;
  displayPath: string;
}

export interface SkippedWorkspace {
  key: string;
  displayPath: string;
  code: ErrorCode;
  message: string;
}

export interface WorkspacesRestoreResult {
  opened: RepoSnapshot[];
  skipped: SkippedWorkspace[];
  activeKey: string | null;
}

export interface WorkspacesSaved {
  saved: boolean;
}

export interface RebasePlanEntry {
  oid: string;
  action: "pick" | "reword" | "squash" | "fixup" | "drop";
  message?: string | null;
}

export interface ClosedResponse {
  closed: boolean;
}

export interface RemovedResponse {
  removed: boolean;
}
