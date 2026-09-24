//! In-memory repository registry: sessions keyed by opaque repo id.
//! Identity (trust cache, write queue in T07) keys on canonical common dir.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::domain::{AppError, ErrorCode, OperationRecord, OperationState, RecoveryAction};
use crate::domain::{RepoState, TrustState};
use crate::git::DiscoveredRepo;

#[derive(Debug, Clone)]
pub struct RepoSession {
    pub repo_id: String,
    pub display_name: String,
    pub display_path: String,
    pub worktree_root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub object_format: String,
    pub trust: TrustState,
    pub version: u64,
}

impl RepoSession {
    /// Stable, exact worktree identity for local UI drafts. This is not an
    /// authorization token and must never be accepted as a Git command path.
    pub fn workspace_key(&self) -> String {
        use std::fmt::Write;
        let mut key = String::from("worktree-v1:");
        for byte in self.worktree_root.as_os_str().as_encoded_bytes() {
            write!(&mut key, "{byte:02x}").expect("write into String");
        }
        key
    }

    /// Stable local key for trust cache + persistence (never the temp repo id).
    pub fn key(&self) -> String {
        self.common_dir.to_string_lossy().into_owned()
    }
}

/// Pinned topology snapshot: tips resolved once per query so pagination
/// cannot skip or duplicate rows while refs move underneath. `truncated`
/// marks a walk cut at the row cap: the cache holds the newest commits
/// only, and pages reaching its end must say so.
#[derive(Debug, Clone)]
pub struct HistorySession {
    pub id: String,
    pub repo_id: String,
    pub tips: Vec<String>,
    pub topo: Vec<crate::git::TopoRow>,
    pub refmap: HashMap<String, Vec<String>>,
    pub truncated: bool,
}

/// Cap on cached history sessions; oldest evicted first.
const MAX_HISTORY_SESSIONS: usize = 32;

/// One cached worktree listing. `generation` bumps on every fresh
/// `repo_status`; path ids embed it (`{repo_id}:{generation}:{index}`) so a
/// token from an older listing is recognizably stale. Raw paths stay bytes.
#[derive(Debug, Clone, Default)]
pub struct StatusListing {
    pub generation: u64,
    pub paths: Vec<Vec<u8>>,
    pub orig_paths: Vec<Option<Vec<u8>>>,
}

/// In-memory job record for the operation coordinator (T07). Terminal
/// records live until repo close or session end; request ids dedup within
/// the session so a double submit returns the same operation.
#[derive(Debug, Clone)]
pub struct JobEntry {
    pub record: OperationRecord,
    /// Opaque fingerprint of the submitting payload; a repeated request id
    /// with a different payload is rejected instead of aliased.
    pub payload_fingerprint: String,
}

#[derive(Debug, Default)]
pub struct RepoRegistry {
    sessions: HashMap<String, RepoSession>,
    history: HashMap<String, HistorySession>,
    history_order: Vec<String>,
    status: HashMap<String, StatusListing>,
    status_generation: u64,
    /// Commit file lists keyed `{repo_id}\0{oid}\0{parent:?}`. Tokens carry a
    /// `c` generation marker so they never validate as worktree path ids.
    commit_files: HashMap<String, StatusListing>,
    commit_generation: u64,
    jobs: HashMap<String, JobEntry>,
    jobs_by_request: HashMap<String, String>,
    /// Per-common-dir write serialization (linked worktrees share one
    /// common dir and therefore one queue). Mutations take the lock for
    /// their session's common dir; reads never do.
    queues: HashMap<String, Arc<AsyncMutex<()>>>,
    confirmations: HashMap<String, ConfirmationEntry>,
    cancel_flags: std::collections::HashSet<String>,
    last_fetch_at: HashMap<String, u64>,
    logs: HashMap<String, std::collections::VecDeque<crate::domain::OperationLogEntry>>,
    log_seq: u64,
    /// Explicit worktree state overrides (`stashConflict`, `merging`).
    /// Absent = `Normal`; cleared on close and on clean mutations.
    repo_states: HashMap<String, RepoState>,
    /// App-origin merges in flight (T13). The pre-merge HEAD and the
    /// conflicting sides live here, outside the repo, so abort can verify
    /// it only undoes what the app started from a clean state.
    merges: HashMap<String, MergeRecord>,
}

/// One app-initiated merge. `MERGE_HEAD` in the repo is the live marker;
/// this record proves the app started it and from which HEAD.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRecord {
    pub pre_head: String,
    pub source_oid: String,
    pub source_label: String,
    pub current_label: String,
}

/// Destructive-action confirmation token (contract §4 `confirmation_prepare`).
/// Single-use, 60s TTL, bound to repo/version/action/targets.
#[derive(Debug, Clone)]
pub struct ConfirmationEntry {
    pub repo_id: String,
    pub version: u64,
    pub action: String,
    pub targets: Vec<String>,
    pub expires_at: u64,
}

/// Confirmation TTL in seconds.
pub const CONFIRMATION_TTL_SECS: u64 = 60;

impl RepoRegistry {
    pub fn open(&mut self, discovered: &DiscoveredRepo, trust: TrustState) -> RepoSession {
        // A linked worktree has its own HEAD/index/status. Only write queues
        // and the trust cache share common-dir identity; tabs must not alias.
        if let Some(existing) = self.sessions.values().find(|s| {
            s.worktree_root == discovered.worktree_root && s.git_dir == discovered.git_dir
        }) {
            return existing.clone();
        }
        let display_path = discovered.worktree_root.to_string_lossy().into_owned();
        let display_name = discovered
            .worktree_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| display_path.clone());
        let session = RepoSession {
            repo_id: Uuid::new_v4().to_string(),
            display_name,
            display_path,
            worktree_root: discovered.worktree_root.clone(),
            git_dir: discovered.git_dir.clone(),
            common_dir: discovered.common_dir.clone(),
            object_format: discovered.object_format.clone(),
            trust,
            version: 1,
        };
        self.sessions
            .insert(session.repo_id.clone(), session.clone());
        session
    }

    pub fn get(&self, repo_id: &str) -> Option<&RepoSession> {
        self.sessions.get(repo_id)
    }

    pub fn get_mut(&mut self, repo_id: &str) -> Option<&mut RepoSession> {
        self.sessions.get_mut(repo_id)
    }

    /// Returns true when a session was actually removed.
    pub fn close(&mut self, repo_id: &str) -> bool {
        self.sessions.remove(repo_id).is_some()
    }

    pub fn bump(&mut self, repo_id: &str) -> Option<u64> {
        let session = self.sessions.get_mut(repo_id)?;
        session.version = session.version.saturating_add(1);
        Some(session.version)
    }

    pub fn state_of(&self, repo_id: &str) -> RepoState {
        self.repo_states
            .get(repo_id)
            .cloned()
            .unwrap_or(RepoState::Normal)
    }

    /// Begin tracking an app-origin merge. Replaces any previous record;
    /// `merge_start` only runs from a clean state so at most one is live.
    pub fn merge_begin(&mut self, repo_id: &str, record: MergeRecord) {
        self.merges.insert(repo_id.to_string(), record);
    }

    pub fn merge_get(&self, repo_id: &str) -> Option<&MergeRecord> {
        self.merges.get(repo_id)
    }

    pub fn merge_clear(&mut self, repo_id: &str) {
        self.merges.remove(repo_id);
    }

    /// Path ids in the latest listing whose raw path equals `raw`.
    /// Conflict commands use this so the UI passes tokens, never paths.
    pub fn status_index_of(&self, repo_id: &str, raw: &[u8]) -> Option<usize> {
        self.status
            .get(repo_id)?
            .paths
            .iter()
            .position(|p| p == raw)
    }

    /// Override the snapshot state (stash conflicts, merges). Mutations
    /// that leave a clean worktree reset it to `Normal`.
    pub fn set_repo_state(&mut self, repo_id: &str, state: RepoState) {
        if state == RepoState::Normal {
            self.repo_states.remove(repo_id);
        } else {
            self.repo_states.insert(repo_id.to_string(), state);
        }
    }

    pub fn history_put(
        &mut self,
        repo_id: &str,
        tips: Vec<String>,
        topo: Vec<crate::git::TopoRow>,
        refmap: HashMap<String, Vec<String>>,
        truncated: bool,
    ) -> HistorySession {
        let session = HistorySession {
            id: uuid::Uuid::new_v4().to_string(),
            repo_id: repo_id.to_string(),
            tips,
            topo,
            refmap,
            truncated,
        };
        self.history_order.push(session.id.clone());
        self.history.insert(session.id.clone(), session.clone());
        while self.history_order.len() > MAX_HISTORY_SESSIONS {
            let oldest = self.history_order.remove(0);
            self.history.remove(&oldest);
        }
        session
    }

    pub fn history_get(&self, session_id: &str) -> Option<&HistorySession> {
        self.history.get(session_id)
    }

    /// Cache a fresh status listing for `repo_id`, returning its generation.
    /// Every call bumps the generation so external changes observed by the
    /// next read invalidate tokens handed out before.
    pub fn status_put(
        &mut self,
        repo_id: &str,
        paths: Vec<Vec<u8>>,
        orig_paths: Vec<Option<Vec<u8>>>,
    ) -> u64 {
        self.status_generation = self.status_generation.saturating_add(1);
        let generation = self.status_generation;
        self.status.insert(
            repo_id.to_string(),
            StatusListing {
                generation,
                paths,
                orig_paths,
            },
        );
        generation
    }

    pub fn status_listing(&self, repo_id: &str) -> Option<&StatusListing> {
        self.status.get(repo_id)
    }

    fn stale_error() -> AppError {
        AppError::new(
            ErrorCode::STALE_STATE,
            "This file list is outdated; refresh the working changes and retry",
            RecoveryAction::Refresh,
            false,
        )
    }

    /// Resolve a path id issued by the latest listing of `repo_id` back to
    /// raw bytes. Tokens from an older generation, another repo, or a
    /// malformed shape are rejected with `STALE_STATE`, never guessed.
    pub fn status_resolve(
        &self,
        repo_id: &str,
        path_id: &str,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), AppError> {
        let listing = self.status.get(repo_id).ok_or_else(Self::stale_error)?;
        let mut parts = path_id.split(':');
        let token_repo = parts.next().ok_or_else(Self::stale_error)?;
        let generation: u64 = parts
            .next()
            .and_then(|g| g.parse().ok())
            .ok_or_else(Self::stale_error)?;
        let index: usize = parts
            .next()
            .and_then(|i| i.parse().ok())
            .ok_or_else(Self::stale_error)?;
        if parts.next().is_some() || token_repo != repo_id || generation != listing.generation {
            return Err(Self::stale_error());
        }
        let path = listing.paths.get(index).ok_or_else(Self::stale_error)?;
        let orig = listing.orig_paths.get(index).cloned().unwrap_or(None);
        Ok((path.clone(), orig))
    }

    /// Path id for `index` in the latest listing of `repo_id`.
    pub fn status_path_id(&self, repo_id: &str, index: usize) -> Option<String> {
        let listing = self.status.get(repo_id)?;
        if index >= listing.paths.len() {
            return None;
        }
        Some(format!("{}:{}:{index}", repo_id, listing.generation))
    }

    fn commit_key(repo_id: &str, oid: &str, parent: Option<usize>) -> String {
        format!(
            "{repo_id}\0{oid}\0{}",
            parent.map_or("-".to_string(), |p| p.to_string())
        )
    }

    /// Cache the file list of one commit + parent comparison, returning the
    /// generation embedded in its tokens.
    pub fn commit_files_put(
        &mut self,
        repo_id: &str,
        oid: &str,
        parent: Option<usize>,
        paths: Vec<Vec<u8>>,
        orig_paths: Vec<Option<Vec<u8>>>,
    ) -> u64 {
        self.commit_generation = self.commit_generation.saturating_add(1);
        let generation = self.commit_generation;
        self.commit_files.insert(
            Self::commit_key(repo_id, oid, parent),
            StatusListing {
                generation,
                paths,
                orig_paths,
            },
        );
        generation
    }

    pub fn commit_path_id(
        &self,
        repo_id: &str,
        oid: &str,
        parent: Option<usize>,
        index: usize,
    ) -> Option<String> {
        let listing = self
            .commit_files
            .get(&Self::commit_key(repo_id, oid, parent))?;
        if index >= listing.paths.len() {
            return None;
        }
        Some(format!("{}:c{}:{index}", repo_id, listing.generation))
    }

    /// Resolve a commit file token to raw `(new, old)` bytes. Only tokens
    /// issued for this exact commit + parent comparison validate; anything
    /// else (including worktree tokens) fails with `STALE_STATE`.
    pub fn commit_file_resolve(
        &self,
        repo_id: &str,
        oid: &str,
        parent: Option<usize>,
        path_id: &str,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), AppError> {
        let listing = self
            .commit_files
            .get(&Self::commit_key(repo_id, oid, parent))
            .ok_or_else(Self::stale_error)?;
        let mut parts = path_id.split(':');
        let token_repo = parts.next().ok_or_else(Self::stale_error)?;
        let generation: u64 = parts
            .next()
            .and_then(|g| g.strip_prefix('c'))
            .and_then(|g| g.parse().ok())
            .ok_or_else(Self::stale_error)?;
        let index: usize = parts
            .next()
            .and_then(|i| i.parse().ok())
            .ok_or_else(Self::stale_error)?;
        if parts.next().is_some() || token_repo != repo_id || generation != listing.generation {
            return Err(Self::stale_error());
        }
        let path = listing.paths.get(index).ok_or_else(Self::stale_error)?;
        let orig = listing.orig_paths.get(index).cloned().unwrap_or(None);
        Ok((path.clone(), orig))
    }

    /// Write queue for a canonical common dir. Linked worktrees sharing one
    /// common dir share one queue, so their mutations serialize.
    pub fn queue_for(&mut self, common_dir_key: &str) -> Arc<AsyncMutex<()>> {
        self.queues
            .entry(common_dir_key.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Issue a single-use confirmation token bound to this exact state.
    pub fn confirmation_issue(
        &mut self,
        repo_id: &str,
        version: u64,
        action: &str,
        targets: Vec<String>,
    ) -> (String, u64) {
        self.confirmations
            .retain(|_, entry| entry.expires_at > Self::now_secs());
        let token = uuid::Uuid::new_v4().to_string();
        let expires_at = Self::now_secs().saturating_add(CONFIRMATION_TTL_SECS);
        self.confirmations.insert(
            token.clone(),
            ConfirmationEntry {
                repo_id: repo_id.to_string(),
                version,
                action: action.to_string(),
                targets,
                expires_at,
            },
        );
        (token, expires_at)
    }

    /// Consume a token, validating every binding. Anything off — unknown,
    /// expired, wrong repo/version/action/targets — fails closed without
    /// telling which binding broke.
    pub fn confirmation_consume(
        &mut self,
        repo_id: &str,
        version: u64,
        action: &str,
        targets: &[String],
        token: &str,
    ) -> Result<(), AppError> {
        let invalid = || {
            AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "This confirmation expired or no longer matches; review and confirm again",
                RecoveryAction::InspectState,
                false,
            )
        };
        let entry = self.confirmations.remove(token).ok_or_else(invalid)?;
        if entry.expires_at <= Self::now_secs()
            || entry.repo_id != repo_id
            || entry.version != version
            || entry.action != action
            || entry.targets != targets
        {
            return Err(invalid());
        }
        Ok(())
    }

    /// Register a job, deduplicating on `request_id`. Same request id with
    /// the same payload fingerprint returns the existing operation; a
    /// different payload is rejected with `INVALID_ARGUMENT`.
    pub fn job_register(
        &mut self,
        record: OperationRecord,
        payload_fingerprint: &str,
    ) -> Result<OperationRecord, AppError> {
        if let Some(existing_id) = self.jobs_by_request.get(&record.request_id) {
            let existing = self.jobs.get(existing_id).ok_or_else(Self::stale_error)?;
            if existing.payload_fingerprint == payload_fingerprint {
                return Ok(existing.record.clone());
            }
            return Err(AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "This request id was already used with different parameters",
                RecoveryAction::InspectState,
                false,
            ));
        }
        let id = record.operation_id.clone();
        self.jobs_by_request
            .insert(record.request_id.clone(), id.clone());
        self.jobs.insert(
            id,
            JobEntry {
                record: record.clone(),
                payload_fingerprint: payload_fingerprint.to_string(),
            },
        );
        Ok(record)
    }

    pub fn job_get(&self, operation_id: &str) -> Option<&OperationRecord> {
        self.jobs.get(operation_id).map(|entry| &entry.record)
    }

    pub fn job_set_state(&mut self, operation_id: &str, state: OperationState) -> bool {
        match self.jobs.get_mut(operation_id) {
            Some(entry) => {
                entry.record.state = state;
                true
            }
            None => false,
        }
    }

    /// True while any non-terminal job belongs to `repo_id`. Close is
    /// rejected while this holds so an unsettled mutation is never orphaned.
    pub fn has_active_operation(&self, repo_id: &str) -> bool {
        self.jobs.values().any(|entry| {
            entry.record.repo_id.as_deref() == Some(repo_id)
                && matches!(
                    entry.record.state,
                    OperationState::Queued | OperationState::Running | OperationState::Cancelling
                )
        })
    }

    /// Request cancellation of a running job. The job task polls this flag
    /// and kills its child; the record moves to `Cancelling` immediately so
    /// the UI never shows a stale Running state.
    pub fn job_request_cancel(&mut self, operation_id: &str) -> bool {
        let active = self
            .jobs
            .get(operation_id)
            .map(|entry| {
                matches!(
                    entry.record.state,
                    OperationState::Queued | OperationState::Running | OperationState::Cancelling
                )
            })
            .unwrap_or(false);
        if active {
            self.job_set_state(operation_id, OperationState::Cancelling);
            self.cancel_flags.insert(operation_id.to_string());
        }
        active
    }

    pub fn job_cancel_requested(&self, operation_id: &str) -> bool {
        self.cancel_flags.contains(operation_id)
    }

    fn job_clear_cancel(&mut self, operation_id: &str) {
        self.cancel_flags.remove(operation_id);
    }

    /// Mark a job terminal and clear its cancel flag.
    pub fn job_finish(
        &mut self,
        operation_id: &str,
        state: OperationState,
        error: Option<AppError>,
    ) -> bool {
        let done = self.job_set_state(operation_id, state);
        if let Some(entry) = self.jobs.get_mut(operation_id) {
            entry.record.error_code = error.as_ref().map(|value| format!("{:?}", value.code));
            entry.record.error = error;
        }
        self.job_clear_cancel(operation_id);
        done
    }

    pub fn job_set_progress(&mut self, operation_id: &str, stage: &str, progress: Option<u32>) {
        if let Some(entry) = self.jobs.get_mut(operation_id) {
            entry.record.stage = stage.to_string();
            entry.record.progress = progress;
        }
    }

    /// Record a successful fetch time (epoch seconds) for `remote_status`.
    pub fn fetch_recorded(&mut self, repo_id: &str, at_secs: u64) {
        self.last_fetch_at.insert(repo_id.to_string(), at_secs);
    }

    pub fn fetch_time(&self, repo_id: &str) -> Option<u64> {
        self.last_fetch_at.get(repo_id).copied()
    }

    /// Append-only operation log (cap 500 per repo, oldest evicted). Callers
    /// must pass already-redacted summaries; entries are never rewritten.
    pub fn log_push(
        &mut self,
        repo_id: &str,
        kind: &str,
        summary: &str,
        outcome: &str,
        error_code: Option<String>,
        at_ms: u64,
    ) {
        self.log_seq = self.log_seq.saturating_add(1);
        let queue = self.logs.entry(repo_id.to_string()).or_default();
        queue.push_back(crate::domain::OperationLogEntry {
            seq: self.log_seq,
            at: at_ms,
            kind: kind.to_string(),
            summary: summary.to_string(),
            outcome: outcome.to_string(),
            error_code,
        });
        while queue.len() > 500 {
            queue.pop_front();
        }
    }

    /// Entries after `cursor` (exclusive), plus the new cursor.
    pub fn log_list(
        &self,
        repo_id: &str,
        cursor: u64,
    ) -> (Vec<crate::domain::OperationLogEntry>, u64) {
        let entries: Vec<crate::domain::OperationLogEntry> = self
            .logs
            .get(repo_id)
            .map(|queue| queue.iter().filter(|e| e.seq > cursor).cloned().collect())
            .unwrap_or_default();
        let next = entries.last().map(|e| e.seq).unwrap_or(cursor);
        (entries, next)
    }

    /// Drop cached history sessions of one repo after ref mutations
    /// (commit/branch): pinned tips would otherwise hide the new topology.
    pub fn invalidate_history(&mut self, repo_id: &str) {
        self.history.retain(|_, session| session.repo_id != repo_id);
        self.history_order
            .retain(|id| self.history.contains_key(id));
    }

    /// Drop per-repo caches (status listings, history sessions, terminal job
    /// records) when the repo closes. Active jobs block close before this.
    pub fn evict_repo(&mut self, repo_id: &str) {
        self.status.remove(repo_id);
        self.repo_states.remove(repo_id);
        self.merges.remove(repo_id);
        self.commit_files
            .retain(|key, _| key.split('\0').next() != Some(repo_id));
        self.history.retain(|_, session| session.repo_id != repo_id);
        self.history_order
            .retain(|id| self.history.contains_key(id));
        let terminal: Vec<String> = self
            .jobs
            .iter()
            .filter(|(_, entry)| entry.record.repo_id.as_deref() == Some(repo_id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in terminal {
            if let Some(entry) = self.jobs.remove(&id) {
                self.jobs_by_request.remove(&entry.record.request_id);
            }
        }
    }
}
