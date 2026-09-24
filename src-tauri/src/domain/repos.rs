//! Repository DTOs (IPC contract §2). `RepoSnapshot` shape matches
//! `src/lib/ipc/types.ts`; counts stay null until the T07 status reader lands.

use serde::{Deserialize, Serialize};

use super::error::{AppError, ErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HeadState {
    Branch {
        ref_id: String,
        name: String,
        oid: String,
    },
    Detached {
        oid: String,
    },
    Unborn {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustState {
    ReadOnly,
    Trusted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepoState {
    Normal,
    Merging,
    StashConflict,
    ExternalOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSnapshot {
    pub repo_id: String,
    pub workspace_key: String,
    pub version: u64,
    pub display_name: String,
    /// Display only; never round-tripped back into a mutation.
    pub display_path: String,
    pub head: HeadState,
    pub trust: TrustState,
    pub state: RepoState,
    pub merge_origin: Option<String>,
    pub upstream: Option<UpstreamInfo>,
    pub last_fetch_at: Option<String>,
    pub active_operation: Option<String>,
    pub staged_count: Option<u64>,
    pub unstaged_count: Option<u64>,
    pub conflict_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfo {
    pub ref_id: String,
    pub ahead: u64,
    pub behind: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefItem {
    pub ref_id: String,
    pub full_name: String,
    pub label: String,
    /// "local" | "remote" | "tag".
    pub kind: String,
    /// Peeled commit oid (annotated tags resolve to their target).
    pub oid: String,
    pub current: bool,
    pub checked_out_elsewhere: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRow {
    pub oid: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub author_name: String,
    pub authored_at: String,
    pub committed_at: String,
    pub refs: Vec<String>,
    pub boundary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub history_session_id: String,
    pub rows: Vec<CommitRow>,
    pub next_cursor: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitFileChange {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
    /// Opaque token into the cached file list of this commit + parent
    /// (contract §3). Added by T08; older clients ignore unknown fields.
    #[serde(default)]
    pub path_id: String,
}

/// Unified diff viewer DTOs (contract §3 `diff_read`). Line `kind` uses the
/// wire strings "context" | "add" | "delete" | "noNewline" directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// Wire values: "context" | "add" | "delete" | "noNewline".
    pub kind: String,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    /// Lossy display text only; never fed back into Git.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    /// Stable identity of the exact raw hunk bytes. Mutations send this
    /// token back instead of sending editable patch text from the UI.
    pub hunk_id: String,
    pub header: String,
    pub old_start: u64,
    pub new_start: u64,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDocument {
    /// "text" | "binary" | "symlink" | "submodule" | "tooLarge".
    pub kind: String,
    pub display_path: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub hunks: Vec<DiffHunk>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Effective commit identity (contract `identity_read`). No secrets: only
/// whether signing is configured, never keys or tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInfo {
    pub name: Option<String>,
    pub email: Option<String>,
    /// "local" | "global" | "system" | "command" | "multiple" | "missing".
    pub scope: String,
    pub signing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub oid: String,
    pub snapshot: Box<RepoSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCreateResult {
    pub snapshot: Box<RepoSnapshot>,
    pub switched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_error: Option<AppError>,
}

/// Effective sync state vs the configured upstream (contract
/// `remote_status`). The URL is always the redacted form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatus {
    pub remote_name: Option<String>,
    pub url: Option<String>,
    pub upstream_ref: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    /// ISO-8601 UTC of the last successful fetch, or null when never.
    pub last_fetch_at: Option<String>,
}

/// Result of delegating a Bitbucket Cloud API token to the user's configured
/// Git credential helper. The token itself never crosses this response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketConnectionResult {
    pub remote_name: String,
    pub username: String,
    pub credential_saved: bool,
}

/// One append-only operation log row. Summaries are pre-redacted; entries
/// are never rewritten, only evicted past the per-repo cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub seq: u64,
    /// Epoch milliseconds.
    pub at: u64,
    pub kind: String,
    pub summary: String,
    /// "ok" | "error" | "cancelled".
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogPage {
    pub entries: Vec<OperationLogEntry>,
    pub next_cursor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStarted {
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationDetails {
    pub confirmation_token: String,
    pub summary: String,
    /// Seconds since Unix epoch when the token expires.
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiffTarget {
    #[serde(rename_all = "camelCase")]
    Worktree { path_id: String },
    #[serde(rename_all = "camelCase")]
    Index { path_id: String },
    #[serde(rename_all = "camelCase")]
    Commit {
        oid: String,
        parent_index: Option<usize>,
        path_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub oid: String,
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub authored_at: String,
    pub committed_at: String,
    pub parents: Vec<String>,
    pub parent_index: Option<usize>,
    pub files: Vec<CommitFileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub rows: Vec<CommitRow>,
    pub next_cursor: Option<String>,
    pub incomplete: bool,
}

/// One worktree entry (IPC contract §2). `path_id` is an opaque token bound
/// to the listing generation that produced it; resolving it against raw
/// bytes server-side keeps non-UTF-8 paths exact and stale tokens rejectable.
/// Display strings are lossy and never round-trip back into a mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path_id: String,
    pub display_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_display_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub kind: String,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusData {
    pub files: Vec<ChangedFile>,
}

/// Operation lifecycle record (IPC contract §5). Progress is null when Git
/// gives no total; the UI renders indeterminate progress in that case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub operation_id: String,
    pub request_id: String,
    pub repo_id: Option<String>,
    pub kind: String,
    pub state: OperationState,
    pub stage: String,
    pub progress: Option<u32>,
    pub error_code: Option<String>,
    /// User-safe structured terminal failure. `error_code` remains for
    /// compatibility with the operation log and older frontends.
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub entry_id: String,
    /// Stable local worktree key; legacy entries may contain common-dir paths.
    pub key: String,
    pub display_path: String,
    /// Seconds since Unix epoch.
    pub last_opened_at: u64,
}

/// One workspace the user left open (T19). `key` is the stable worktree key
/// from `RepoSnapshot.workspace_key`; the backend decodes it back to a path
/// on restore and re-validates the repository before opening.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenWorkspaceEntry {
    pub key: String,
    pub display_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedWorkspace {
    pub key: String,
    pub display_path: String,
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesRestoreResult {
    pub opened: Vec<RepoSnapshot>,
    pub skipped: Vec<SkippedWorkspace>,
    pub active_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesSaved {
    pub saved: bool,
}
