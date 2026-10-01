//! `merge_start` / `merge_complete` / `merge_abort` plus the conflict
//! inspector (`conflict_list`, `conflict_preview`, `conflict_accept`,
//! `conflict_mark_resolved`, `conflict_mark_all_resolved`) — T13 — and the merge editor
//! (`conflict_hunks`, `conflict_merge`), which resolves text conflicts
//! per block or per line from working-file markers.
//!
//! Merge and conflict safety rules:
//!
//! - Merge starts only from a branch HEAD, a tracked-clean worktree/index,
//!   no active operation and no merge already in flight. The source is an
//!   exact validated ref or OID. `--no-ff --no-commit` guarantees a review
//!   stop; already-up-to-date is a no-op success. No autostash, no
//!   unrelated histories.
//! - The pre-merge HEAD and the app-origin marker live in the registry,
//!   outside the repo. Abort is offered only for app-origin merges from a
//!   clean state, behind a confirmation token, and never falls back to
//!   reset/clean when `merge --abort` fails.
//! - Conflicts read the unmerged index stages (base=1, current=2,
//!   incoming=3). A missing stage is valid (add/add, modify/delete) —
//!   never synthesized. Whole-file accept covers supported regular-file
//!   text conflicts only: stage bytes replace the worktree file atomically
//!   without following symlinks, behind a confirmation token and a working
//!   fingerprint. Symlink/submodule/binary/rename cases get external
//!   guidance, never a wrong write. Only mark-resolved stages the index;
//!   nothing auto-commits.
//! - Complete requires a live `MERGE_HEAD`, the expected HEAD, zero
//!   unmerged entries and a reviewed staged result; the merge commit must
//!   have two parents.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, ErrorCode, RecoveryAction, RepoSnapshot, RepoState, RequestId, TrustState,
};
use crate::git::{
    commit_staged, has_unmerged, read_identity, runner::GitOutput, CommitError, GitRunner,
    READ_TIMEOUT, WRITE_TIMEOUT,
};
use crate::services::{MergeRecord, RepoRegistry, RepoSession};

const MAX_SUBJECT_CHARS: usize = 500;
const MAX_BODY_BYTES: usize = 64 * 1024;
const PREVIEW_CAP_BYTES: usize = 32 * 1024;
/// Merge-editor files above this resolve externally instead.
const MERGE_CAP_BYTES: usize = 256 * 1024;
/// Merged output above this is refused; picks reference existing lines,
/// so legitimate output stays near input size.
const MERGE_OUTPUT_CAP_BYTES: usize = 4 * 1024 * 1024;
const MAX_MERGE_PICKS: usize = 200_000;
const MAX_BULK_RESOLVE_PATHS: usize = 5_000;
const MAX_BULK_RESOLVE_BYTES: usize = 512 * 1024;

fn bad_request(message: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::INVALID_ARGUMENT,
        message,
        RecoveryAction::InspectState,
        false,
    )
}

fn check_request_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 128 {
        return Err(bad_request("requestId must be 1..=128 characters"));
    }
    Ok(())
}

fn session_missing() -> AppError {
    AppError::new(
        ErrorCode::REPO_UNAVAILABLE,
        "Repository session is no longer open",
        RecoveryAction::ChooseRepository,
        false,
    )
}

fn trust_required(action: &str) -> AppError {
    AppError::new(
        ErrorCode::TRUST_REQUIRED,
        format!("Trust this repository to {action}"),
        RecoveryAction::InspectState,
        false,
    )
}

fn stale_state() -> AppError {
    AppError::new(
        ErrorCode::STALE_STATE,
        "The repository changed under you; refresh and retry",
        RecoveryAction::Refresh,
        false,
    )
}

fn git_runner() -> Result<GitRunner, AppError> {
    GitRunner::resolve_from_path()
        .map(GitRunner::new)
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::GIT_NOT_FOUND,
                "Git executable not found on PATH",
                RecoveryAction::ConfigureGit,
                false,
            )
        })
}

fn commit_error(_error: CommitError) -> AppError {
    git_failed()
}

fn git_failed() -> AppError {
    AppError::new(
        ErrorCode::GIT_ERROR,
        "Git could not finish the merge operation",
        RecoveryAction::Refresh,
        true,
    )
}

/// One conflicted path with its surviving stages.
#[derive(Debug, Clone, PartialEq, Eq)]
struct UnmergedEntry {
    path: Vec<u8>,
    /// (mode, oid) for stages 1..=3; `None` = stage absent (valid).
    stages: [Option<(String, String)>; 4],
}

impl UnmergedEntry {
    fn present(&self) -> [bool; 4] {
        let mut out = [false; 4];
        for (stage, slot) in self.stages.iter().enumerate().skip(1) {
            out[stage] = slot.is_some();
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path_id: String,
    pub display_path: String,
    /// "text" | "addAdd" | "modifyDelete" | "binary" | "symlink" |
    /// "submodule" | "unsupported".
    pub kind: String,
    pub has_base: bool,
    pub has_current: bool,
    pub has_incoming: bool,
    pub supported: bool,
    pub support_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConflictFile {
    pub display_path: String,
    pub original_path: Option<String>,
    /// First `git diff --name-status` status letter: M/A/D/R/C/T.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictList {
    pub files: Vec<ConflictFile>,
    pub resolved_files: Vec<ResolvedConflictFile>,
    pub current_label: String,
    pub incoming_label: String,
    pub can_complete: bool,
    pub can_abort: bool,
    pub abort_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagePreview {
    /// Lossy text of the stage blob, truncated at the preview cap.
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPreview {
    pub path_id: String,
    pub display_path: String,
    pub base: Option<StagePreview>,
    pub current: Option<StagePreview>,
    pub incoming: Option<StagePreview>,
    pub working_fingerprint: String,
    pub supported_actions: Vec<String>,
    pub support_reason: Option<String>,
    pub current_label: String,
    pub incoming_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStartResult {
    pub snapshot: RepoSnapshot,
    pub conflicted: bool,
    pub already_up_to_date: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCompleteResult {
    pub oid: String,
    pub snapshot: RepoSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAcceptResult {
    pub snapshot: RepoSnapshot,
    pub working_fingerprint: String,
}

async fn run_git(
    runner: &GitRunner,
    cwd: &std::path::Path,
    argv: &[String],
    timeout: std::time::Duration,
) -> Result<GitOutput, AppError> {
    runner
        .run(cwd, argv, timeout)
        .await
        .map_err(|_| git_failed())
}

async fn run_git_os(
    runner: &GitRunner,
    cwd: &std::path::Path,
    argv: &[OsString],
    timeout: std::time::Duration,
) -> Result<GitOutput, AppError> {
    runner
        .run(cwd, argv, timeout)
        .await
        .map_err(|_| git_failed())
}

fn check_write_context(
    registry: &RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    action: &str,
) -> Result<RepoSession, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required(action));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    if registry.has_active_operation(repo_id) {
        return Err(AppError::new(
            ErrorCode::REPO_BUSY,
            "A background operation is still running for this repository",
            RecoveryAction::InspectState,
            false,
        ));
    }
    Ok(session)
}

async fn fresh_snapshot(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
) -> Result<RepoSnapshot, AppError> {
    registry.bump(repo_id).ok_or_else(session_missing)?;
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    super::repos::build_snapshot(runner, registry, &session).await
}

fn merge_head_live(session: &RepoSession) -> bool {
    session.git_dir.join("MERGE_HEAD").exists()
}

async fn head_branch(runner: &GitRunner, session: &RepoSession) -> Result<String, AppError> {
    let out = run_git(
        runner,
        &session.worktree_root,
        &[
            "symbolic-ref".to_string(),
            "-q".to_string(),
            "--short".to_string(),
            "HEAD".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(bad_request(
            "Merges start from a branch HEAD; detached or unborn HEAD cannot merge",
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn head_oid(runner: &GitRunner, session: &RepoSession) -> Result<String, AppError> {
    let out = run_git(
        runner,
        &session.worktree_root,
        &["rev-parse".to_string(), "HEAD".to_string()],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(bad_request("Cannot read HEAD"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Tracked worktree + index clean. Untracked files never block a merge;
/// `git merge` only refuses when they would be overwritten.
async fn tracked_clean(runner: &GitRunner, session: &RepoSession) -> Result<bool, AppError> {
    let worktree = run_git(
        runner,
        &session.worktree_root,
        &["diff".to_string(), "--quiet".to_string()],
        READ_TIMEOUT,
    )
    .await?;
    if !worktree.success {
        return Ok(false);
    }
    let index = run_git(
        runner,
        &session.worktree_root,
        &[
            "diff".to_string(),
            "--cached".to_string(),
            "--quiet".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    Ok(index.success)
}

/// Raw unmerged index entries (`ls-files -u -z`, quotepath off for byte
/// exactness), grouped by path.
async fn read_unmerged(
    runner: &GitRunner,
    session: &RepoSession,
) -> Result<Vec<UnmergedEntry>, AppError> {
    let out = run_git(
        runner,
        &session.worktree_root,
        &[
            "-c".to_string(),
            "core.quotepath=off".to_string(),
            "ls-files".to_string(),
            "-u".to_string(),
            "-z".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(git_failed());
    }
    let mut entries: Vec<UnmergedEntry> = Vec::new();
    for record in out.stdout.split(|b| *b == 0) {
        if record.is_empty() {
            continue;
        }
        let tab = record
            .iter()
            .position(|b| *b == b'\t')
            .ok_or_else(git_failed)?;
        let (meta, path) = record.split_at(tab);
        let path = &path[1..];
        let mut fields = meta.split(|b| *b == b' ');
        let (Some(mode), Some(oid), Some(stage)) = (fields.next(), fields.next(), fields.next())
        else {
            return Err(git_failed());
        };
        if fields.next().is_some() {
            return Err(git_failed());
        }
        let stage: usize = std::str::from_utf8(stage)
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|s| (1..=3).contains(s))
            .ok_or_else(git_failed)?;
        let mode = String::from_utf8_lossy(mode).into_owned();
        let oid = String::from_utf8_lossy(oid).into_owned();
        match entries.iter_mut().find(|e| e.path == path) {
            Some(entry) => entry.stages[stage] = Some((mode, oid)),
            None => {
                let mut stages: [Option<(String, String)>; 4] = [None, None, None, None];
                stages[stage] = Some((mode, oid));
                entries.push(UnmergedEntry {
                    path: path.to_vec(),
                    stages,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Stage-0 paths already included in the pending merge result. App-started
/// merges begin from a clean index, so `HEAD..index` is the resolved-file
/// list users expect. Unmerged entries are filtered defensively.
async fn read_resolved_merge_files(
    runner: &GitRunner,
    session: &RepoSession,
    unmerged: &[UnmergedEntry],
) -> Result<Vec<ResolvedConflictFile>, AppError> {
    let out = run_git(
        runner,
        &session.worktree_root,
        &[
            "-c".to_string(),
            "core.quotepath=off".to_string(),
            "diff".to_string(),
            "--cached".to_string(),
            "--name-status".to_string(),
            "-z".to_string(),
            "--find-renames".to_string(),
            "HEAD".to_string(),
            "--".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(git_failed());
    }

    let blocked: HashSet<&[u8]> = unmerged.iter().map(|entry| entry.path.as_slice()).collect();
    let fields: Vec<&[u8]> = out
        .stdout
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect();
    let mut files = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let raw_status = fields[index];
        index += 1;
        let status = raw_status.first().copied().ok_or_else(git_failed)?;
        let renamed = status == b'R' || status == b'C';
        let original = if renamed {
            let path = fields.get(index).copied().ok_or_else(git_failed)?;
            index += 1;
            Some(path)
        } else {
            None
        };
        let path = fields.get(index).copied().ok_or_else(git_failed)?;
        index += 1;
        if status == b'U' || blocked.contains(path) {
            continue;
        }
        files.push(ResolvedConflictFile {
            display_path: String::from_utf8_lossy(path).into_owned(),
            original_path: original.map(|value| String::from_utf8_lossy(value).into_owned()),
            status: char::from(status).to_string(),
        });
    }
    files.sort_by(|a, b| a.display_path.cmp(&b.display_path));
    Ok(files)
}

/// Blob bytes of one surviving stage (`:[stage]:path`, byte-exact).
async fn stage_bytes(
    runner: &GitRunner,
    session: &RepoSession,
    stage: usize,
    path: &[u8],
) -> Result<Vec<u8>, AppError> {
    let mut spec = OsString::from(format!(":{stage}:"));
    spec.push(OsStr::from_bytes(path));
    let out = run_git_os(
        runner,
        &session.worktree_root,
        &[OsString::from("show"), spec],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(git_failed());
    }
    Ok(out.stdout)
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

fn kind_of(entry: &UnmergedEntry, stage_text: &[Option<bool>]) -> (&'static str, bool, String) {
    // Modes decide symlink/submodule first; NUL bytes decide binary.
    let mut modes: Vec<&str> = Vec::new();
    for (mode, _) in entry.stages.iter().flatten() {
        modes.push(mode.as_str());
    }
    if modes.contains(&"120000") {
        return (
            "symlink",
            false,
            "Symlink conflicts need an external tool: inspect both targets, fix the link, then mark resolved.".to_string(),
        );
    }
    if modes.contains(&"160000") {
        return (
            "submodule",
            false,
            "Submodule conflicts need an external tool: pick the commit inside the submodule, then mark resolved.".to_string(),
        );
    }
    if modes.iter().any(|m| !m.starts_with("100")) {
        return (
            "unsupported",
            false,
            "Unusual file modes collided here; resolve this file outside the app, then mark resolved.".to_string(),
        );
    }
    if stage_text.contains(&Some(false)) {
        return (
            "binary",
            false,
            "Binary files cannot be merged line-wise: keep one side with an external tool (or craft the bytes), then mark resolved.".to_string(),
        );
    }
    let present = entry.present();
    if !present[1] && present[2] && present[3] {
        // Add/add is still whole-file resolvable when both sides are text.
        return ("text", true, String::new());
    }
    if present[1] {
        if !present[2] || !present[3] {
            return ("modifyDelete", true, String::new());
        }
        return ("text", true, String::new());
    }
    (
        "unsupported",
        false,
        "Missing base with an unusual stage mix; resolve outside the app, then mark resolved."
            .to_string(),
    )
}

/// FNV-1a over the worktree bytes plus length: cheap change detector, not a
/// security hash. Recomputed at accept/resolve time.
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= bytes.len() as u64;
    hash = hash.wrapping_mul(0x100000001b3);
    format!("{hash:016x}:{}", bytes.len())
}

fn worktree_path(session: &RepoSession, raw: &[u8]) -> Result<std::path::PathBuf, AppError> {
    let rel = std::ffi::OsStr::from_bytes(raw);
    let candidate = session.worktree_root.join(rel);
    if !candidate.starts_with(&session.worktree_root) {
        return Err(bad_request("Conflict path escapes the worktree"));
    }
    Ok(candidate)
}

/// Resolve a UI path id to live unmerged bytes. The token must come from
/// the current status listing AND the path must still be unmerged.
async fn resolve_conflict_path(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
    session: &RepoSession,
    path_id: &str,
) -> Result<(Vec<u8>, UnmergedEntry), AppError> {
    let (raw, _) = registry
        .status_resolve(repo_id, path_id)
        .map_err(|_| stale_state())?;
    let live = read_unmerged(runner, session).await?;
    let entry = live
        .into_iter()
        .find(|e| e.path == raw)
        .ok_or_else(stale_state)?;
    Ok((raw, entry))
}

/// Text-ness per surviving stage (`None` = absent). Errors only on Git
/// failure, never on content.
async fn stage_textness(
    runner: &GitRunner,
    session: &RepoSession,
    entry: &UnmergedEntry,
) -> Result<[Option<bool>; 4], AppError> {
    let mut out: [Option<bool>; 4] = [None, None, None, None];
    for (stage, slot) in entry.stages.iter().enumerate().skip(1) {
        if slot.is_some() {
            let bytes = stage_bytes(runner, session, stage, &entry.path).await?;
            out[stage] = Some(!is_binary(&bytes));
        }
    }
    Ok(out)
}

fn preview_of(bytes: &[u8]) -> StagePreview {
    let truncated = bytes.len() > PREVIEW_CAP_BYTES;
    let slice = if truncated {
        &bytes[..PREVIEW_CAP_BYTES]
    } else {
        bytes
    };
    StagePreview {
        text: String::from_utf8_lossy(slice).into_owned(),
        truncated,
    }
}

fn side_labels(registry: &RepoRegistry, repo_id: &str, fallback_current: &str) -> (String, String) {
    match registry.merge_get(repo_id) {
        Some(record) => (record.current_label.clone(), record.source_label.clone()),
        None => (fallback_current.to_string(), "incoming".to_string()),
    }
}

async fn core_list(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
) -> Result<ConflictList, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let live_merge = merge_head_live(&session);
    let entries = read_unmerged(runner, &session).await?;
    let mut files = Vec::with_capacity(entries.len());
    for entry in &entries {
        let textness = stage_textness(runner, &session, entry).await?;
        let (kind, supported, reason) = kind_of(entry, &textness);
        let display = String::from_utf8_lossy(&entry.path).into_owned();
        let path_id = match registry.status_index_of(repo_id, &entry.path) {
            Some(index) => registry
                .status_path_id(repo_id, index)
                .ok_or_else(stale_state)?,
            None => return Err(stale_state()),
        };
        let present = entry.present();
        files.push(ConflictFile {
            path_id,
            display_path: display,
            kind: kind.to_string(),
            has_base: present[1],
            has_current: present[2],
            has_incoming: present[3],
            supported,
            support_reason: if supported { None } else { Some(reason) },
        });
    }
    let resolved_files = if live_merge {
        read_resolved_merge_files(runner, &session, &entries).await?
    } else {
        Vec::new()
    };
    let (current_label, incoming_label) = if live_merge {
        side_labels(registry, repo_id, &session.display_name)
    } else {
        (String::new(), String::new())
    };
    let record = registry.merge_get(repo_id).cloned();
    let can_complete = live_merge && entries.is_empty();
    let (can_abort, abort_reason) = match (&record, live_merge) {
        (Some(_), true) => (true, None),
        (Some(_), false) => (
            false,
            Some("The merge marker is gone; the merge already ended.".to_string()),
        ),
        (None, true) => (
            false,
            Some(
                "This merge was not started by the app; finish or abort it in the terminal so nothing unrecoverable is lost."
                    .to_string(),
            ),
        ),
        (None, false) => (false, None),
    };
    Ok(ConflictList {
        files,
        resolved_files,
        current_label,
        incoming_label,
        can_complete,
        can_abort,
        abort_reason,
    })
}

async fn core_preview(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
    path_id: &str,
) -> Result<ConflictPreview, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let (raw, entry) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let textness = stage_textness(runner, &session, &entry).await?;
    let (_, supported, reason) = kind_of(&entry, &textness);
    let mut previews: [Option<StagePreview>; 4] = [None, None, None, None];
    if supported {
        for (stage, slot) in entry.stages.iter().enumerate().skip(1) {
            if slot.is_some() {
                let bytes = stage_bytes(runner, &session, stage, &raw).await?;
                previews[stage] = Some(preview_of(&bytes));
            }
        }
    }
    let target = worktree_path(&session, &raw)?;
    let working = std::fs::read(&target).unwrap_or_default();
    let (current_label, incoming_label) = side_labels(registry, repo_id, &session.display_name);
    let display = String::from_utf8_lossy(&raw).into_owned();
    let mut actions = Vec::new();
    if supported {
        // Only sides whose stage survived are offerable; a missing side
        // means deletion, which resolves through mark-resolved instead.
        if entry.stages[2].is_some() {
            actions.push("current".to_string());
        }
        if entry.stages[3].is_some() {
            actions.push("incoming".to_string());
        }
    }
    Ok(ConflictPreview {
        path_id: path_id.to_string(),
        display_path: display,
        base: previews[1].clone(),
        current: previews[2].clone(),
        incoming: previews[3].clone(),
        working_fingerprint: fingerprint(&working),
        supported_actions: actions,
        support_reason: if supported { None } else { Some(reason) },
        current_label,
        incoming_label,
    })
}

/// One parsed conflict block: raw side lines (endings preserved) plus the
/// verbatim block bytes for fail-closed identity and passthrough.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MergeBlock {
    id: String,
    current: Vec<Vec<u8>>,
    incoming: Vec<Vec<u8>>,
    base: Vec<Vec<u8>>,
    has_base: bool,
    raw: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MergeSegment {
    Clean { bytes: Vec<u8> },
    Conflict(MergeBlock),
}

/// Split keeping `\n` on each line; the last line may lack it.
fn split_raw_lines(bytes: &[u8]) -> Vec<&[u8]> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            out.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

fn strip_eol(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn is_opener(s: &[u8]) -> bool {
    s.len() >= 7 && &s[..7] == b"<<<<<<<"
}

fn is_base_sep(s: &[u8]) -> bool {
    s.len() >= 7 && &s[..7] == b"|||||||"
}

fn is_sep(s: &[u8]) -> bool {
    s == b"======="
}

fn is_closer(s: &[u8]) -> bool {
    s.len() >= 7 && &s[..7] == b">>>>>>>"
}

/// Parse working-file conflict markers (`merge` and `diff3` styles) into
/// clean regions and conflict blocks. First separator wins, matching git
/// tooling; anything structurally malformed fails closed so the file
/// resolves externally instead of splicing the wrong bytes.
fn parse_merge_file(bytes: &[u8]) -> Result<Vec<MergeSegment>, AppError> {
    #[derive(PartialEq, Eq)]
    enum Scan {
        Clean,
        Current,
        Base,
        Incoming,
    }
    let malformed = || {
        bad_request(
            "Conflict markers are malformed; resolve this file outside the app, then mark resolved",
        )
    };
    let mut segments = Vec::new();
    let mut clean: Vec<u8> = Vec::new();
    let mut block: Option<MergeBlock> = None;
    let mut scan = Scan::Clean;
    let mut saw_sep = false;
    for line in split_raw_lines(bytes) {
        let s = strip_eol(line);
        if scan == Scan::Clean {
            if is_opener(s) {
                if !clean.is_empty() {
                    segments.push(MergeSegment::Clean {
                        bytes: std::mem::take(&mut clean),
                    });
                }
                block = Some(MergeBlock {
                    id: String::new(),
                    current: Vec::new(),
                    incoming: Vec::new(),
                    base: Vec::new(),
                    has_base: false,
                    raw: Vec::new(),
                });
                scan = Scan::Current;
                saw_sep = false;
                block.as_mut().expect("block").raw.extend_from_slice(line);
            } else {
                clean.extend_from_slice(line);
            }
            continue;
        }
        let b = block.as_mut().expect("block");
        b.raw.extend_from_slice(line);
        if is_opener(s) {
            return Err(malformed());
        }
        if scan == Scan::Current && is_base_sep(s) {
            b.has_base = true;
            scan = Scan::Base;
        } else if (scan == Scan::Current || scan == Scan::Base) && is_sep(s) {
            scan = Scan::Incoming;
            saw_sep = true;
        } else if is_closer(s) {
            if !saw_sep {
                return Err(malformed());
            }
            b.id = fingerprint(&b.raw);
            segments.push(MergeSegment::Conflict(block.take().expect("block")));
            scan = Scan::Clean;
        } else {
            match scan {
                Scan::Current => b.current.push(line.to_vec()),
                Scan::Base => b.base.push(line.to_vec()),
                Scan::Incoming => b.incoming.push(line.to_vec()),
                Scan::Clean => unreachable!("clean handled above"),
            }
        }
    }
    if scan != Scan::Clean {
        return Err(malformed());
    }
    if !clean.is_empty() {
        segments.push(MergeSegment::Clean { bytes: clean });
    }
    Ok(segments)
}

fn display_line(raw: &[u8]) -> String {
    String::from_utf8_lossy(strip_eol(raw)).into_owned()
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MergeSegmentDto {
    Clean {
        lines: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Conflict {
        hunk_id: String,
        current: Vec<String>,
        incoming: Vec<String>,
        base: Vec<String>,
        raw: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictHunks {
    pub path_id: String,
    pub display_path: String,
    pub current_label: String,
    pub incoming_label: String,
    pub working_fingerprint: String,
    pub segments: Vec<MergeSegmentDto>,
    pub working_text: String,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePickedLine {
    pub side: String,
    pub index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeBlockPick {
    pub hunk_id: String,
    pub lines: Vec<MergePickedLine>,
}

fn segment_dto(segment: &MergeSegment) -> MergeSegmentDto {
    match segment {
        MergeSegment::Clean { bytes } => MergeSegmentDto::Clean {
            lines: split_raw_lines(bytes)
                .iter()
                .map(|l| display_line(l))
                .collect(),
        },
        MergeSegment::Conflict(block) => MergeSegmentDto::Conflict {
            hunk_id: block.id.clone(),
            current: block.current.iter().map(|l| display_line(l)).collect(),
            incoming: block.incoming.iter().map(|l| display_line(l)).collect(),
            base: block.base.iter().map(|l| display_line(l)).collect(),
            raw: String::from_utf8_lossy(&block.raw).into_owned(),
        },
    }
}

async fn core_hunks(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
    path_id: &str,
) -> Result<ConflictHunks, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let (raw, entry) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let textness = stage_textness(runner, &session, &entry).await?;
    let (kind, supported, _) = kind_of(&entry, &textness);
    if !supported || kind != "text" {
        return Err(bad_request(
            "The merge editor covers supported text conflicts only; use whole-file accept or mark resolved after an external resolution",
        ));
    }
    let target = worktree_path(&session, &raw)?;
    let working = std::fs::read(&target)
        .map_err(|_| bad_request("Working file is missing; refresh and preview again"))?;
    if working.len() > MERGE_CAP_BYTES {
        return Err(bad_request(
            "This file is too large for the merge editor; resolve it outside the app, then mark resolved",
        ));
    }
    let segments = parse_merge_file(&working)?;
    let conflict_count = segments
        .iter()
        .filter(|s| matches!(s, MergeSegment::Conflict(_)))
        .count();
    let (current_label, incoming_label) = side_labels(registry, repo_id, &session.display_name);
    Ok(ConflictHunks {
        path_id: path_id.to_string(),
        display_path: String::from_utf8_lossy(&raw).into_owned(),
        current_label,
        incoming_label,
        working_fingerprint: fingerprint(&working),
        segments: segments.iter().map(segment_dto).collect(),
        working_text: String::from_utf8(working.clone())
            .map_err(|_| bad_request("The editor requires UTF-8 text"))?,
        conflict_count,
    })
}

/// Temporary merge inputs are private and never replace the working file/index.
struct AutoMergeInputs(std::path::PathBuf);
impl AutoMergeInputs {
    fn new() -> Result<Self, AppError> {
        use std::os::unix::fs::DirBuilderExt;
        let path = std::env::temp_dir().join(format!("octopus-auto-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| git_failed())?;
        Ok(Self(path))
    }
}
impl Drop for AutoMergeInputs {
    fn drop(&mut self) {
        for name in ["base", "current", "incoming"] {
            let _ = std::fs::remove_file(self.0.join(name));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

type BlockSides = (Vec<Vec<u8>>, Vec<Vec<u8>>);
type BlockBases = std::collections::HashMap<BlockSides, Option<Vec<Vec<u8>>>>;

fn record_base(
    bases: &mut BlockBases,
    current: Vec<Vec<u8>>,
    incoming: Vec<Vec<u8>>,
    base: Vec<Vec<u8>>,
) {
    bases
        .entry((current, incoming))
        .and_modify(|known| {
            if known.as_ref() != Some(&base) {
                *known = None;
            }
        })
        .or_insert(Some(base));
}

fn index_block_bases(segments: Vec<MergeSegment>) -> BlockBases {
    let mut bases = BlockBases::new();
    for segment in segments {
        let MergeSegment::Conflict(mut block) = segment else {
            continue;
        };
        if !block.has_base {
            continue;
        }
        record_base(
            &mut bases,
            block.current.clone(),
            block.incoming.clone(),
            block.base.clone(),
        );
        // Merge-style markers may factor unchanged common edges out of a diff3
        // block. Trim only edges also present in the base; never infer a base
        // from shared additions or an ambiguous match.
        let prefix = block
            .base
            .iter()
            .zip(&block.current)
            .zip(&block.incoming)
            .take_while(|((base, current), incoming)| base == current && base == incoming)
            .count();
        block.current.drain(..prefix);
        block.incoming.drain(..prefix);
        block.base.drain(..prefix);
        while !block.base.is_empty()
            && block.current.last() == block.base.last()
            && block.incoming.last() == block.base.last()
        {
            block.current.pop();
            block.incoming.pop();
            block.base.pop();
        }
        record_base(&mut bases, block.current, block.incoming, block.base);
    }
    bases
}

async fn read_block_bases(
    runner: &GitRunner,
    session: &RepoSession,
    entry: &UnmergedEntry,
) -> Result<BlockBases, AppError> {
    if !(1..=3).all(|stage| entry.stages[stage].is_some()) {
        return Ok(BlockBases::new());
    }
    let inputs = AutoMergeInputs::new()?;
    for (stage, name) in [(1, "base"), (2, "current"), (3, "incoming")] {
        let bytes = stage_bytes(runner, session, stage, &entry.path).await?;
        if bytes.len() > MERGE_CAP_BYTES || is_binary(&bytes) {
            return Ok(BlockBases::new());
        }
        std::fs::write(inputs.0.join(name), bytes).map_err(|_| git_failed())?;
    }
    let output = runner
        .run(
            &inputs.0,
            &[
                "merge-file",
                "--stdout",
                "--diff3",
                "--quiet",
                "--marker-size=7",
                "-L",
                "current",
                "-L",
                "base",
                "-L",
                "incoming",
                "current",
                "base",
                "incoming",
            ],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed())?;
    // A conflict is an expected nonzero exit. Actual failures have stderr and
    // must not be treated as an empty/clean merge.
    if !output.success && (!output.stderr.is_empty() || output.stdout.is_empty()) {
        return Err(git_failed());
    }
    if output.stdout.len() > MERGE_OUTPUT_CAP_BYTES {
        return Ok(BlockBases::new());
    }
    Ok(index_block_bases(parse_merge_file(&output.stdout)?))
}

async fn core_auto_resolve(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    request: &ConflictAutoResolveRequest,
) -> Result<ConflictAutoResolveResult, AppError> {
    let session = check_write_context(
        registry,
        &request.repo_id,
        request.expected_version,
        "resolve conflicts",
    )?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;
    let (raw, entry) = resolve_conflict_path(
        runner,
        registry,
        &request.repo_id,
        &session,
        &request.path_id,
    )
    .await?;
    let target = worktree_path(&session, &raw)?;
    if !std::fs::symlink_metadata(&target)
        .map_err(|_| git_failed())?
        .is_file()
    {
        return Err(bad_request("Auto resolve requires a regular working file"));
    }
    let doc = core_hunks(runner, registry, &request.repo_id, &request.path_id).await?;
    if doc.working_fingerprint != request.working_fingerprint {
        return Err(stale_state());
    }
    let segments = parse_merge_file(doc.working_text.as_bytes())?;
    let needs_base = segments.iter().any(|segment| matches!(segment, MergeSegment::Conflict(block) if !block.has_base && block.current != block.incoming));
    let bases = if needs_base {
        read_block_bases(runner, &session, &entry).await?
    } else {
        BlockBases::new()
    };
    let mut picks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for segment in segments {
        let MergeSegment::Conflict(block) = segment else {
            continue;
        };
        let recovered = bases
            .get(&(block.current.clone(), block.incoming.clone()))
            .and_then(Option::as_deref);
        let base = if block.has_base {
            Some(block.base.as_slice())
        } else {
            recovered
        };
        if let Some(lines) = crate::git::auto_merge::resolve(base, &block.current, &block.incoming)
        {
            if seen.insert(block.id.clone()) {
                picks.push(MergeBlockPick {
                    hunk_id: block.id,
                    lines: lines
                        .into_iter()
                        .map(|line| match line {
                            crate::git::auto_merge::Pick::Current(index) => MergePickedLine {
                                side: "current".into(),
                                index,
                            },
                            crate::git::auto_merge::Pick::Incoming(index) => MergePickedLine {
                                side: "incoming".into(),
                                index,
                            },
                        })
                        .collect(),
                });
            }
        }
    }
    let working = std::fs::read(worktree_path(&session, &raw)?).map_err(|_| git_failed())?;
    if fingerprint(&working) != request.working_fingerprint {
        return Err(stale_state());
    }
    let (_, latest) = resolve_conflict_path(
        runner,
        registry,
        &request.repo_id,
        &session,
        &request.path_id,
    )
    .await?;
    if latest.stages != entry.stages {
        return Err(stale_state());
    }
    Ok(ConflictAutoResolveResult {
        working_fingerprint: doc.working_fingerprint,
        picks,
    })
}

async fn core_merge(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request: &ConflictMergeRequest,
) -> Result<ConflictMergeResult, AppError> {
    let path_id = request.path_id.as_str();
    if request.result_text.is_some() && !request.picks.is_empty() {
        return Err(bad_request(
            "Send either edited text or line selections, not both",
        ));
    }
    if request.result_text.is_none() && request.picks.is_empty() {
        return Err(bad_request("Select at least one block or line first"));
    }
    let total_picks: usize = request.picks.iter().map(|p| p.lines.len()).sum();
    if total_picks > MAX_MERGE_PICKS {
        return Err(bad_request(
            "Too many picked lines; resolve in smaller steps",
        ));
    }
    let session = check_write_context(
        registry,
        repo_id,
        request.expected_version,
        "resolve conflicts",
    )?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let (raw, entry) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let textness = stage_textness(runner, &session, &entry).await?;
    let (kind, supported, _) = kind_of(&entry, &textness);
    if !supported || kind != "text" {
        return Err(bad_request(
            "The merge editor covers supported text conflicts only",
        ));
    }
    let target = worktree_path(&session, &raw)?;
    let meta = std::fs::symlink_metadata(&target)
        .map_err(|_| bad_request("Working file is missing; refresh and preview again"))?;
    if !meta.is_file() {
        return Err(bad_request(
            "Working path is not a regular file; resolve it outside the app, then mark resolved",
        ));
    }
    let working = std::fs::read(&target).map_err(|_| git_failed())?;
    if fingerprint(&working) != request.working_fingerprint {
        return Err(stale_state());
    }
    if working.len() > MERGE_CAP_BYTES {
        return Err(bad_request(
            "This file is too large for the merge editor; resolve it outside the app, then mark resolved",
        ));
    }
    let segments = parse_merge_file(&working)?;
    let mut blocks = std::collections::HashMap::new();
    for segment in &segments {
        if let MergeSegment::Conflict(block) = segment {
            blocks.insert(block.id.clone(), block);
        }
    }
    // Picks reference block identities issued by conflict_hunks; anything
    // unknown or out of range means the file drifted underneath the UI.
    let mut seen = std::collections::HashSet::new();
    let mut picked: std::collections::HashMap<&str, &MergeBlockPick> =
        std::collections::HashMap::new();
    for pick in &request.picks {
        if pick.hunk_id.is_empty() || pick.hunk_id.len() > 128 {
            return Err(bad_request("Invalid hunk identity"));
        }
        if !seen.insert(pick.hunk_id.as_str()) {
            return Err(bad_request("Duplicate hunk in one resolution"));
        }
        let block = blocks.get(pick.hunk_id.as_str()).ok_or_else(stale_state)?;
        for line in &pick.lines {
            let side = match line.side.as_str() {
                "current" => &block.current,
                "incoming" => &block.incoming,
                _ => return Err(bad_request("Picked side must be current or incoming")),
            };
            if line.index >= side.len() {
                return Err(stale_state());
            }
        }
        picked.insert(pick.hunk_id.as_str(), pick);
    }
    let mut out: Vec<u8> = Vec::new();
    for segment in &segments {
        match segment {
            MergeSegment::Clean { bytes } => out.extend_from_slice(bytes),
            MergeSegment::Conflict(block) => match picked.get(block.id.as_str()) {
                None => out.extend_from_slice(&block.raw),
                Some(pick) => {
                    for line in &pick.lines {
                        let side = if line.side == "current" {
                            &block.current
                        } else {
                            &block.incoming
                        };
                        out.extend_from_slice(&side[line.index]);
                    }
                }
            },
        }
        if out.len() > MERGE_OUTPUT_CAP_BYTES {
            return Err(bad_request(
                "Merged output is too large; resolve in smaller steps",
            ));
        }
    }
    let (resolved_blocks, remaining_blocks) = if let Some(text) = &request.result_text {
        if text.len() > MERGE_CAP_BYTES || text.contains('\0') {
            return Err(bad_request(
                "Edited result must be UTF-8 text under 256 KiB without NUL bytes",
            ));
        }
        // Validate retained markers before writing so partial edits can be reopened.
        let edited = parse_merge_file(text.as_bytes())?;
        let remaining = edited
            .iter()
            .filter(|s| matches!(s, MergeSegment::Conflict(_)))
            .count();
        out = text.as_bytes().to_vec();
        (blocks.len().saturating_sub(remaining), remaining)
    } else {
        (picked.len(), blocks.len() - picked.len())
    };
    // Create a private sibling, preserve executable bits, then atomically replace.
    // A pre-existing file/symlink must never become an output target.
    use std::io::Write;
    static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp = target.clone().into_os_string();
    tmp.push(format!(".octopus-merge-{}-{serial}", std::process::id()));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|_| git_failed())?;
    let write_result = (|| {
        output.write_all(&out).map_err(|_| git_failed())?;
        output
            .set_permissions(meta.permissions())
            .map_err(|_| git_failed())?;
        output.sync_all().map_err(|_| git_failed())?;
        if fingerprint(&std::fs::read(&target).map_err(|_| git_failed())?)
            != request.working_fingerprint
        {
            return Err(stale_state());
        }
        std::fs::rename(&tmp, &target).map_err(|_| git_failed())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_result?;
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(ConflictMergeResult {
        snapshot,
        working_fingerprint: fingerprint(&out),
        resolved_blocks,
        remaining_blocks,
    })
}

/// Summary for the `conflict_accept` confirmation token. The token targets
/// `[pathId, side]`; consume must see the exact same pair.
pub(crate) async fn accept_summary(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
    path_id: &str,
    side: &str,
) -> Result<String, AppError> {
    if side != "current" && side != "incoming" {
        return Err(bad_request("Resolution side must be current or incoming"));
    }
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let (raw, entry) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let textness = stage_textness(runner, &session, &entry).await?;
    let (kind, supported, _) = kind_of(&entry, &textness);
    if !supported || kind != "text" {
        return Err(bad_request(
            "Whole-file accept covers supported text conflicts only; use mark-resolved after an external resolution",
        ));
    }
    let stage = if side == "current" { 2 } else { 3 };
    let bytes = match entry.stages[stage] {
        Some(_) => stage_bytes(runner, &session, stage, &raw).await?,
        None => {
            return Err(bad_request(
                "That side deleted the file; resolve it as a deletion instead",
            ));
        }
    };
    let (current_label, incoming_label) = side_labels(registry, repo_id, &session.display_name);
    let label = if side == "current" {
        current_label
    } else {
        incoming_label
    };
    Ok(format!(
        "Overwrite working file '{}' with the {} version ({} bytes, {} stage). The current working content is lost; the file stays unstaged.",
        String::from_utf8_lossy(&raw),
        label,
        bytes.len(),
        side,
    ))
}

async fn core_accept(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request: &ConflictAcceptRequest,
) -> Result<ConflictAcceptResult, AppError> {
    let side = request.side.as_str();
    let path_id = request.path_id.as_str();
    if side != "current" && side != "incoming" {
        return Err(bad_request("Resolution side must be current or incoming"));
    }
    let session = check_write_context(
        registry,
        repo_id,
        request.expected_version,
        "resolve conflicts",
    )?;
    registry.confirmation_consume(
        repo_id,
        session.version,
        "conflict_accept",
        &[path_id.to_string(), side.to_string()],
        &request.confirmation_token,
    )?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let (raw, entry) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let textness = stage_textness(runner, &session, &entry).await?;
    let (kind, supported, _) = kind_of(&entry, &textness);
    // Validation refusals happen before any write, so the version (and
    // sibling tokens) stay untouched.
    if !supported || kind != "text" {
        return Err(bad_request(
            "Whole-file accept covers supported text conflicts only",
        ));
    }
    let stage = if side == "current" { 2 } else { 3 };
    let bytes = match entry.stages[stage] {
        Some(_) => stage_bytes(runner, &session, stage, &raw).await?,
        None => {
            return Err(bad_request(
                "That side deleted the file; resolve it as a deletion instead",
            ));
        }
    };
    let target = worktree_path(&session, &raw)?;
    // Never follow a symlink out of the worktree.
    let meta = std::fs::symlink_metadata(&target)
        .map_err(|_| bad_request("Working file is missing; refresh and preview again"))?;
    if !meta.is_file() {
        return Err(bad_request(
            "Working path is not a regular file; resolve it outside the app, then mark resolved",
        ));
    }
    let current = std::fs::read(&target).map_err(|_| git_failed())?;
    if fingerprint(&current) != request.working_fingerprint {
        return Err(stale_state());
    }
    // Atomic replace in the same directory; the file stays unstaged.
    let mut tmp = target.clone().into_os_string();
    tmp.push(".gitdock-tmp");
    std::fs::write(&tmp, &bytes).map_err(|_| git_failed())?;
    if std::fs::rename(&tmp, &target).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(git_failed());
    }
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(ConflictAcceptResult {
        snapshot,
        working_fingerprint: fingerprint(&bytes),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkResolution {
    WorkingFile,
    Deletion,
}

async fn core_mark_resolved(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    path_id: &str,
    working_fingerprint: &str,
    resolution: MarkResolution,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "resolve conflicts")?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let (raw, _) = resolve_conflict_path(runner, registry, repo_id, &session, path_id).await?;
    let target = worktree_path(&session, &raw)?;
    match resolution {
        MarkResolution::WorkingFile => {
            let meta = std::fs::symlink_metadata(&target).map_err(|_| {
                bad_request("Working file is missing; it cannot be staged as resolved")
            })?;
            if !meta.is_file() {
                return Err(bad_request(
                    "Only regular files stage through mark-resolved",
                ));
            }
            let current = std::fs::read(&target).map_err(|_| git_failed())?;
            if fingerprint(&current) != working_fingerprint {
                return Err(stale_state());
            }
        }
        MarkResolution::Deletion => {
            // A confirmed deletion: the file must already be gone. The UI
            // confirms the removal itself; the backend never deletes here.
            if target.exists() {
                return Err(bad_request(
                    "The file still exists; remove it first (with explicit UI confirmation), then mark resolved as deletion",
                ));
            }
        }
    }
    // Stage exactly this path, byte-exact; nothing else moves.
    let mut spec = OsString::from(":(");
    spec.push("literal)");
    spec.push(OsStr::from_bytes(&raw));
    let out = run_git_os(
        runner,
        &session.worktree_root,
        &[OsString::from("add"), OsString::from("--"), spec],
        WRITE_TIMEOUT,
    )
    .await?;
    if !out.success {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed());
    }
    // Still a merge until complete; the state override stays.
    fresh_snapshot(runner, registry, repo_id).await
}

/// Stage every currently unmerged path exactly once. All targets are checked
/// before the single Git mutation so a symlink, submodule, or unusual path
/// cannot leave a half-resolved index.
async fn core_mark_all_resolved(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "resolve conflicts")?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;
    let entries = read_unmerged(runner, &session).await?;
    if entries.is_empty() {
        return Err(bad_request("There are no unresolved conflicts to mark"));
    }
    if entries.len() > MAX_BULK_RESOLVE_PATHS
        || entries.iter().map(|entry| entry.path.len()).sum::<usize>() > MAX_BULK_RESOLVE_BYTES
    {
        return Err(bad_request(
            "Too many conflict paths to mark in one operation; resolve them in smaller groups",
        ));
    }

    for entry in &entries {
        let target = worktree_path(&session, &entry.path)?;
        match std::fs::symlink_metadata(&target) {
            Ok(meta) if meta.is_file() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(bad_request(
                    "Mark all supports regular files and confirmed deletions only; resolve symlink and submodule conflicts individually",
                ))
            }
        }
    }

    let mut argv = vec![
        OsString::from("add"),
        OsString::from("-A"),
        OsString::from("--"),
    ];
    for entry in &entries {
        let mut spec = OsString::from(":(literal)");
        spec.push(OsStr::from_bytes(&entry.path));
        argv.push(spec);
    }
    let out = run_git_os(runner, &session.worktree_root, &argv, WRITE_TIMEOUT).await?;
    if !out.success {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed());
    }
    fresh_snapshot(runner, registry, repo_id).await
}

/// Validate the merge source: a live ref (local, remote-tracking or tag)
/// or a 40-hex commit OID. Returns the commit OID plus a UI label.
async fn resolve_source(
    runner: &GitRunner,
    session: &RepoSession,
    source_ref_id: &str,
) -> Result<(String, String), AppError> {
    if source_ref_id.is_empty() || source_ref_id.len() > 512 {
        return Err(bad_request("Unknown merge source"));
    }
    let refs_out = run_git(
        runner,
        &session.worktree_root,
        &[
            "for-each-ref".to_string(),
            "--format=%(refname)".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !refs_out.success {
        return Err(git_failed());
    }
    let decoded = String::from_utf8_lossy(&refs_out.stdout).into_owned();
    let live: Vec<&str> = decoded.lines().map(str::trim).collect();
    let is_hex_oid =
        source_ref_id.len() == 40 && source_ref_id.bytes().all(|b| b.is_ascii_hexdigit());
    let (candidate, label) = if live.contains(&source_ref_id) {
        let label = source_ref_id
            .rsplit('/')
            .next()
            .unwrap_or(source_ref_id)
            .to_string();
        (source_ref_id.to_string(), label)
    } else if is_hex_oid {
        let label = source_ref_id.chars().take(12).collect();
        (source_ref_id.to_string(), label)
    } else {
        return Err(bad_request(
            "Merge source must be a current branch, tag or commit",
        ));
    };
    // Pin the exact commit now; the merge runs against the OID, not the ref.
    let resolved = run_git(
        runner,
        &session.worktree_root,
        &[
            "rev-parse".to_string(),
            "--verify".to_string(),
            format!("{candidate}^{{commit}}"),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !resolved.success {
        return Err(bad_request("Merge source does not resolve to a commit"));
    }
    Ok((
        String::from_utf8_lossy(&resolved.stdout).trim().to_string(),
        label,
    ))
}

async fn core_start(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    source_ref_id: &str,
    confirmed_target_oid: &str,
) -> Result<MergeStartResult, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "start merges")?;
    if merge_head_live(&session) {
        return Err(bad_request(
            "A merge is already in progress; complete or abort it first",
        ));
    }
    let current = head_branch(runner, &session).await?;
    let head = head_oid(runner, &session).await?;
    if head != confirmed_target_oid {
        return Err(stale_state());
    }
    if !tracked_clean(runner, &session).await? {
        return Err(AppError::new(
            ErrorCode::DIRTY_WORKTREE,
            "Merges start from a clean worktree; stash or commit first",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let (source_oid, source_label) = resolve_source(runner, &session, source_ref_id).await?;
    // Already-up-to-date (including source == HEAD) is a no-op success.
    let up_to_date = if source_oid == head {
        true
    } else {
        let base = run_git(
            runner,
            &session.worktree_root,
            &[
                "merge-base".to_string(),
                "HEAD".to_string(),
                source_oid.clone(),
            ],
            READ_TIMEOUT,
        )
        .await?;
        if !base.success {
            return Err(bad_request(
                "Unrelated histories are never merged automatically",
            ));
        }
        run_git(
            runner,
            &session.worktree_root,
            &[
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                source_oid.clone(),
                "HEAD".to_string(),
            ],
            READ_TIMEOUT,
        )
        .await?
        .success
    };
    if up_to_date {
        registry.bump(repo_id).ok_or_else(session_missing)?;
        let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
        let snapshot = super::repos::build_snapshot(runner, registry, &session).await?;
        return Ok(MergeStartResult {
            snapshot,
            conflicted: false,
            already_up_to_date: true,
        });
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    registry.merge_begin(
        repo_id,
        MergeRecord {
            pre_head: head,
            source_oid: source_oid.clone(),
            source_label: source_label.clone(),
            current_label: current.clone(),
        },
    );
    registry.set_repo_state(repo_id, RepoState::Merging);
    let merged = run_git(
        runner,
        &session.worktree_root,
        &[
            "merge".to_string(),
            "--no-ff".to_string(),
            "--no-commit".to_string(),
            "--no-edit".to_string(),
            source_oid.clone(),
        ],
        WRITE_TIMEOUT,
    )
    .await?;
    if merged.success {
        registry.invalidate_history(repo_id);
        let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
        return Ok(MergeStartResult {
            snapshot,
            conflicted: false,
            already_up_to_date: false,
        });
    }
    // A failed merge with unmerged entries is the normal conflict path.
    // Anything else restores the clean pre-merge state via `merge --abort`;
    // a failed abort keeps the Merging state and reports, never resets.
    let unmerged = has_unmerged(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?;
    if !unmerged {
        let aborted = run_git(
            runner,
            &session.worktree_root,
            &["merge".to_string(), "--abort".to_string()],
            WRITE_TIMEOUT,
        )
        .await?;
        if aborted.success && !merge_head_live(&session) {
            registry.merge_clear(repo_id);
            registry.set_repo_state(repo_id, RepoState::Normal);
            let _ = fresh_snapshot(runner, registry, repo_id).await;
            return Err(git_failed());
        }
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            "The merge failed and automatic cleanup did not finish; inspect the repository before retrying",
            RecoveryAction::InspectState,
            true,
        ));
    }
    registry.invalidate_history(repo_id);
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(MergeStartResult {
        snapshot,
        conflicted: true,
        already_up_to_date: false,
    })
}

async fn core_complete(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request: &MergeCompleteRequest,
) -> Result<MergeCompleteResult, AppError> {
    let subject = request.subject.as_str();
    let body = request.body.as_str();
    if subject.trim().is_empty() {
        return Err(bad_request("A merge commit needs a subject"));
    }
    if subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err(bad_request("Merge subject exceeds 500 characters"));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(bad_request("Merge body exceeds 64 KiB"));
    }
    if !request.staged_reviewed {
        return Err(bad_request(
            "Review the staged merge result before completing",
        ));
    }
    let session = check_write_context(
        registry,
        repo_id,
        request.expected_version,
        "complete merges",
    )?;
    if !merge_head_live(&session) {
        return Err(bad_request("No merge is in progress"));
    }
    let head = head_oid(runner, &session).await?;
    if head != request.confirmed_head_oid {
        return Err(stale_state());
    }
    let unmerged = has_unmerged(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?;
    if unmerged {
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "Unmerged files remain; resolve every conflict before completing",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    let identity = read_identity(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?;
    if identity.name.as_deref().unwrap_or("").is_empty()
        || identity.email.as_deref().unwrap_or("").is_empty()
    {
        return Err(AppError::new(
            ErrorCode::IDENTITY_MISSING,
            "Git identity is missing: set user.name and user.email in your Git config, then retry",
            RecoveryAction::ConfigureGit,
            false,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let mut message = String::with_capacity(subject.len() + body.len() + 4);
    message.push_str(subject.trim());
    let body = body.trim();
    if !body.is_empty() {
        message.push_str("\n\n");
        message.push_str(body);
    }
    message.push('\n');
    let oid = commit_staged(runner, &session.worktree_root, message.as_bytes())
        .await
        .map_err(commit_error)?;
    // The merge commit must have exactly two parents.
    let parents = run_git(
        runner,
        &session.worktree_root,
        &[
            "rev-list".to_string(),
            "--parents".to_string(),
            "-n".to_string(),
            "1".to_string(),
            "HEAD".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    let count = String::from_utf8_lossy(&parents.stdout)
        .split_whitespace()
        .count();
    if !parents.success || count != 3 {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed());
    }
    registry.merge_clear(repo_id);
    registry.set_repo_state(repo_id, RepoState::Normal);
    registry.invalidate_history(repo_id);
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(MergeCompleteResult { oid, snapshot })
}

/// Summary for the `merge_abort` confirmation token (targets `["merge"]`).
pub(crate) async fn abort_summary(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
) -> Result<String, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let record = registry
        .merge_get(repo_id)
        .cloned()
        .ok_or_else(|| bad_request("Only merges started by the app can be aborted here"))?;
    if !merge_head_live(&session) {
        return Err(bad_request(
            "The merge marker is gone; the merge already ended",
        ));
    }
    let head = head_oid(runner, &session).await?;
    Ok(format!(
        "Abort the app-started merge of '{}' into '{}'? The worktree returns to {} as recorded before the merge. Edits made after the merge started cannot be restored by abort; current HEAD is {}.",
        record.source_label,
        record.current_label,
        record.pre_head.chars().take(12).collect::<String>(),
        head.chars().take(12).collect::<String>(),
    ))
}

async fn core_abort(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "abort merges")?;
    registry.confirmation_consume(
        repo_id,
        session.version,
        "merge_abort",
        &["merge".to_string()],
        confirmation_token,
    )?;
    // Re-verify app origin after the token: only app-started merges abort.
    let record = registry
        .merge_get(repo_id)
        .cloned()
        .ok_or_else(|| bad_request("Only merges started by the app can be aborted here"))?;
    if !merge_head_live(&session) {
        return Err(bad_request(
            "The merge marker is gone; the merge already ended",
        ));
    }
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let aborted = run_git(
        runner,
        &session.worktree_root,
        &["merge".to_string(), "--abort".to_string()],
        WRITE_TIMEOUT,
    )
    .await?;
    // No fallback reset/clean: a failed abort keeps everything for manual
    // recovery and says so.
    if !aborted.success || merge_head_live(&session) {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            "Aborting the merge did not finish; the merge state was kept for manual recovery",
            RecoveryAction::InspectState,
            true,
        ));
    }
    // The worktree must be back at the recorded pre-merge HEAD.
    let head = head_oid(runner, &session).await?;
    if head != record.pre_head {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            "Abort finished on an unexpected HEAD; inspect the repository before continuing",
            RecoveryAction::InspectState,
            true,
        ));
    }
    registry.merge_clear(repo_id);
    registry.set_repo_state(repo_id, RepoState::Normal);
    registry.invalidate_history(repo_id);
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeContext {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeStartRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub source_ref_id: String,
    pub confirmed_target_oid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCompleteRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub staged_reviewed: bool,
    pub confirmed_head_oid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAbortRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub confirmation_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPreviewRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub path_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAcceptRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub side: String,
    pub working_fingerprint: String,
    pub confirmation_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolveRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub working_fingerprint: String,
    pub resolution: MarkResolution,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictResolveAllRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictHunksRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub path_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictMergeRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub working_fingerprint: String,
    pub picks: Vec<MergeBlockPick>,
    #[serde(default)]
    pub result_text: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictMergeResult {
    pub snapshot: RepoSnapshot,
    pub working_fingerprint: String,
    pub resolved_blocks: usize,
    pub remaining_blocks: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAutoResolveRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub working_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictAutoResolveResult {
    pub working_fingerprint: String,
    pub picks: Vec<MergeBlockPick>,
}

// ---- Commands ----

#[tauri::command]
pub async fn conflict_list(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: MergeContext,
) -> Result<ApiResult<ConflictList>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_list(&runner, &mut registry, &request.repo_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_preview(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictPreviewRequest,
) -> Result<ApiResult<ConflictPreview>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        core_preview(&runner, &registry, &request.repo_id, &request.path_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_accept(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictAcceptRequest,
) -> Result<ApiResult<ConflictAcceptResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_accept(&runner, &mut registry, &request.repo_id, &request).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_mark_resolved(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictResolveRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_mark_resolved(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.path_id,
            &request.working_fingerprint,
            request.resolution,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_mark_all_resolved(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictResolveAllRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_mark_all_resolved(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_hunks(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictHunksRequest,
) -> Result<ApiResult<ConflictHunks>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        core_hunks(&runner, &registry, &request.repo_id, &request.path_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_auto_resolve(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictAutoResolveRequest,
) -> Result<ApiResult<ConflictAutoResolveResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_auto_resolve(&runner, &mut registry, &request).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn conflict_merge(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConflictMergeRequest,
) -> Result<ApiResult<ConflictMergeResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_merge(&runner, &mut registry, &request.repo_id, &request).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn merge_start(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: MergeStartRequest,
) -> Result<ApiResult<MergeStartResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_start(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.source_ref_id,
            &request.confirmed_target_oid,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn merge_complete(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: MergeCompleteRequest,
) -> Result<ApiResult<MergeCompleteResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_complete(&runner, &mut registry, &request.repo_id, &request).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn merge_abort(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: MergeAbortRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_abort(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.confirmation_token,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

pub mod prelude {
    pub use super::{
        conflict_accept, conflict_auto_resolve, conflict_hunks, conflict_list,
        conflict_mark_all_resolved, conflict_mark_resolved, conflict_merge, conflict_preview,
        merge_abort, merge_complete, merge_start, ConflictAcceptRequest, ConflictAcceptResult,
        ConflictFile, ConflictHunks, ConflictHunksRequest, ConflictList, ConflictMergeRequest,
        ConflictMergeResult, ConflictPreview, ConflictPreviewRequest, ConflictResolveAllRequest,
        ConflictResolveRequest, MarkResolution, MergeAbortRequest, MergeCompleteRequest,
        MergeCompleteResult, MergeContext, MergeStartRequest, MergeStartResult,
        ResolvedConflictFile, StagePreview,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@x")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@x")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn head_of(repo: &std::path::Path) -> String {
        String::from_utf8(
            StdCommand::new("git")
                .current_dir(repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse")
                .stdout,
        )
        .expect("utf8")
        .trim()
        .to_string()
    }

    fn parents_of(repo: &std::path::Path) -> usize {
        String::from_utf8(
            StdCommand::new("git")
                .current_dir(repo)
                .args(["rev-list", "--parents", "-n", "1", "HEAD"])
                .output()
                .expect("rev-list")
                .stdout,
        )
        .expect("utf8")
        .split_whitespace()
        .count()
            - 1
    }

    /// main + base commit + local identity. Returns (dir, repo).
    fn temp_repo(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("gitdock-t13-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        std::fs::write(repo.join("base.txt"), "base\n").expect("write");
        git(&repo, &["add", "base.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        (dir, repo)
    }

    fn open_repo(registry: &mut RepoRegistry, repo: &std::path::Path) -> String {
        let discovered = crate::git::DiscoveredRepo {
            worktree_root: repo.to_path_buf(),
            git_dir: repo.join(".git"),
            common_dir: repo.join(".git"),
            object_format: "sha1".to_string(),
            bare: false,
        };
        registry.open(&discovered, TrustState::Trusted).repo_id
    }

    fn version_of(registry: &RepoRegistry, repo_id: &str) -> u64 {
        registry.get(repo_id).expect("session").version
    }

    /// Fresh status listing, exactly like the UI reload. Path ids issued
    /// before this call are stale by design.
    async fn live_status(runner: &GitRunner, registry: &mut RepoRegistry, repo_id: &str) {
        let session = registry.get(repo_id).expect("session").clone();
        let parsed = crate::git::read_status(runner, &session.worktree_root)
            .await
            .expect("status");
        let raws: Vec<Vec<u8>> = parsed.files.iter().map(|f| f.path.clone()).collect();
        let origs: Vec<Option<Vec<u8>>> =
            parsed.files.iter().map(|f| f.orig_path.clone()).collect();
        registry.status_put(repo_id, raws, origs);
    }

    fn confirm_token(
        registry: &mut RepoRegistry,
        repo_id: &str,
        version: u64,
        action: &str,
        targets: Vec<String>,
    ) -> String {
        registry
            .confirmation_issue(repo_id, version, action, targets)
            .0
    }

    /// main vs feature diverge on the SAME line of the SAME file.
    fn diverge_content(repo: &std::path::Path) {
        git(repo, &["checkout", "-b", "feature"]);
        std::fs::write(repo.join("f.txt"), "base\nfeature\n").expect("write");
        git(repo, &["add", "f.txt"]);
        git(repo, &["commit", "-m", "feature side"]);
        git(repo, &["checkout", "main"]);
        std::fs::write(repo.join("f.txt"), "base\nmain\n").expect("write");
        git(repo, &["add", "f.txt"]);
        git(repo, &["commit", "-m", "main side"]);
    }

    async fn start(
        runner: &GitRunner,
        registry: &mut RepoRegistry,
        repo_id: &str,
        source: &str,
        target: &str,
    ) -> MergeStartResult {
        let v = version_of(registry, repo_id);
        core_start(runner, registry, repo_id, v, source, target)
            .await
            .expect("start")
    }

    #[tokio::test]
    async fn clean_merge_completes_with_two_parents() {
        let (_dir, repo) = temp_repo("clean");
        git(&repo, &["checkout", "-b", "feature"]);
        std::fs::write(repo.join("feat.txt"), "new\n").expect("write");
        git(&repo, &["add", "feat.txt"]);
        git(&repo, &["commit", "-m", "feature"]);
        git(&repo, &["checkout", "main"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(!started.conflicted && !started.already_up_to_date);
        assert_eq!(registry.state_of(&repo_id), RepoState::Merging);

        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert!(list.files.is_empty());
        assert!(list.can_complete && list.can_abort);

        let v = version_of(&registry, &repo_id);
        let done = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: true,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect("complete");
        assert_eq!(parents_of(&repo), 2);
        assert_eq!(done.oid.len(), 40);
        assert_eq!(registry.state_of(&repo_id), RepoState::Normal);
        assert!(registry.merge_get(&repo_id).is_none());
        assert_eq!(done.snapshot.merge_origin, None);
    }

    #[tokio::test]
    async fn already_up_to_date_is_a_noop() {
        let (_dir, repo) = temp_repo("uptodate");
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(&runner, &mut registry, &repo_id, "refs/heads/main", &target).await;
        assert!(started.already_up_to_date && !started.conflicted);
        assert!(!repo.join(".git").join("MERGE_HEAD").exists());
    }

    #[tokio::test]
    async fn dirty_worktree_and_unrelated_histories_refuse() {
        let (_dir, repo) = temp_repo("dirty");
        git(&repo, &["checkout", "-b", "feature"]);
        std::fs::write(repo.join("feat.txt"), "new\n").expect("write");
        git(&repo, &["add", "feat.txt"]);
        git(&repo, &["commit", "-m", "feature"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("base.txt"), "dirty\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let v = version_of(&registry, &repo_id);
        let err = core_start(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "refs/heads/feature",
            &target,
        )
        .await
        .expect_err("dirty must refuse");
        assert_eq!(err.code, ErrorCode::DIRTY_WORKTREE);

        git(&repo, &["checkout", "--", "base.txt"]);
        git(&repo, &["checkout", "--orphan", "stranger"]);
        std::fs::write(repo.join("s.txt"), "s\n").expect("write");
        git(&repo, &["add", "s.txt"]);
        git(&repo, &["commit", "-m", "stranger"]);
        git(&repo, &["checkout", "main"]);
        let v = version_of(&registry, &repo_id);
        let err = core_start(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "refs/heads/stranger",
            &target,
        )
        .await
        .expect_err("unrelated must refuse");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        let v = version_of(&registry, &repo_id);
        let err = core_start(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "refs/heads/nope",
            &target,
        )
        .await
        .expect_err("unknown source must refuse");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn content_conflict_accept_mark_complete_flow() {
        let (_dir, repo) = temp_repo("content");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        diverge_content(&repo);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(started.conflicted);
        assert_eq!(started.snapshot.state, RepoState::Merging);

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(list.files.len(), 1);
        let file = &list.files[0];
        assert_eq!(file.kind, "text");
        assert!(file.supported && file.has_base && file.has_current && file.has_incoming);
        assert!(!list.can_complete);

        let preview = core_preview(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("preview");
        assert_eq!(
            preview.supported_actions,
            vec!["current".to_string(), "incoming".to_string()]
        );
        assert!(preview.current.as_ref().unwrap().text.contains("main"));
        assert!(preview.incoming.as_ref().unwrap().text.contains("feature"));
        assert_eq!(preview.current_label, "main");
        assert_eq!(preview.incoming_label, "feature");

        // Accept incoming behind its confirmation token.
        let v = version_of(&registry, &repo_id);
        let token = confirm_token(
            &mut registry,
            &repo_id,
            v,
            "conflict_accept",
            vec![file.path_id.clone(), "incoming".to_string()],
        );
        let accepted = core_accept(
            &runner,
            &mut registry,
            &repo_id,
            &ConflictAcceptRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                path_id: file.path_id.to_string(),
                side: ("incoming").to_string(),
                working_fingerprint: preview.working_fingerprint.to_string(),
                confirmation_token: token.to_string(),
            },
        )
        .await
        .expect("accept");
        assert_eq!(
            std::fs::read(repo.join("f.txt")).expect("read"),
            b"base\nfeature\n"
        );
        // Updated but still unmerged: only mark-resolved stages.
        let still = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(still.files.len(), 1);

        // Mark resolved stages exactly this file; the merge stays open.
        live_status(&runner, &mut registry, &repo_id).await;
        let relist = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        let v = version_of(&registry, &repo_id);
        let snap = core_mark_resolved(
            &runner,
            &mut registry,
            &repo_id,
            v,
            &relist.files[0].path_id,
            &accepted.working_fingerprint,
            MarkResolution::WorkingFile,
        )
        .await
        .expect("mark resolved");
        assert_eq!(snap.state, RepoState::Merging);
        let empty = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert!(empty.files.is_empty() && empty.can_complete);

        // Complete needs a reviewed staged result and the expected HEAD.
        let v = version_of(&registry, &repo_id);
        let err = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: false,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect_err("unreviewed must refuse");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
        let done = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: true,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect("complete");
        assert_eq!(parents_of(&repo), 2);
        assert_eq!(done.oid.len(), 40);
    }

    #[tokio::test]
    async fn stale_fingerprint_and_missing_side_refuse() {
        let (_dir, repo) = temp_repo("stale-fp");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        diverge_content(&repo);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        let file = &list.files[0];

        let v = version_of(&registry, &repo_id);
        let token = confirm_token(
            &mut registry,
            &repo_id,
            v,
            "conflict_accept",
            vec![file.path_id.clone(), "current".to_string()],
        );
        let err = core_accept(
            &runner,
            &mut registry,
            &repo_id,
            &ConflictAcceptRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                path_id: file.path_id.to_string(),
                side: ("current").to_string(),
                working_fingerprint: ("deadbeef:1").to_string(),
                confirmation_token: token.to_string(),
            },
        )
        .await
        .expect_err("stale fingerprint must refuse");
        assert_eq!(err.code, ErrorCode::STALE_STATE);
    }

    #[tokio::test]
    async fn add_add_conflict_resolves_whole_file() {
        let (_dir, repo) = temp_repo("addadd");
        git(&repo, &["checkout", "-b", "feature"]);
        std::fs::write(repo.join("new.txt"), "feature version\n").expect("write");
        git(&repo, &["add", "new.txt"]);
        git(&repo, &["commit", "-m", "feature adds"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("new.txt"), "main version\n").expect("write");
        git(&repo, &["add", "new.txt"]);
        git(&repo, &["commit", "-m", "main adds"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(started.conflicted);

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(list.files.len(), 1);
        // No base stage, both sides text: still whole-file resolvable.
        assert!(!list.files[0].has_base);
        assert!(list.files[0].supported);
        let preview = core_preview(&runner, &registry, &repo_id, &list.files[0].path_id)
            .await
            .expect("preview");
        assert!(preview.base.is_none());
        assert_eq!(
            preview.supported_actions,
            vec!["current".to_string(), "incoming".to_string()]
        );

        let v = version_of(&registry, &repo_id);
        let token = confirm_token(
            &mut registry,
            &repo_id,
            v,
            "conflict_accept",
            vec![list.files[0].path_id.clone(), "current".to_string()],
        );
        core_accept(
            &runner,
            &mut registry,
            &repo_id,
            &ConflictAcceptRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                path_id: list.files[0].path_id.to_string(),
                side: ("current").to_string(),
                working_fingerprint: preview.working_fingerprint.to_string(),
                confirmation_token: token.to_string(),
            },
        )
        .await
        .expect("accept current");
        assert_eq!(
            std::fs::read(repo.join("new.txt")).expect("read"),
            b"main version\n"
        );
    }

    #[tokio::test]
    async fn modify_delete_offers_deletion_not_side_accept() {
        let (_dir, repo) = temp_repo("moddel");
        std::fs::write(repo.join("gone.txt"), "v1\n").expect("write");
        git(&repo, &["add", "gone.txt"]);
        git(&repo, &["commit", "-m", "add gone"]);
        git(&repo, &["checkout", "-b", "feature"]);
        git(&repo, &["rm", "gone.txt"]);
        git(&repo, &["commit", "-m", "feature deletes"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("gone.txt"), "v2\n").expect("write");
        git(&repo, &["add", "gone.txt"]);
        git(&repo, &["commit", "-m", "main modifies"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(started.conflicted);

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(list.files.len(), 1);
        assert_eq!(list.files[0].kind, "modifyDelete");
        assert!(!list.files[0].has_incoming);
        let preview = core_preview(&runner, &registry, &repo_id, &list.files[0].path_id)
            .await
            .expect("preview");
        // The deleted side is not offerable; deletion resolves explicitly.
        assert_eq!(preview.supported_actions, vec!["current".to_string()]);

        // Accepting the missing side refuses and keeps everything.
        let summary = accept_summary(
            &runner,
            &registry,
            &repo_id,
            &list.files[0].path_id,
            "incoming",
        )
        .await
        .expect_err("missing side must refuse");
        assert_eq!(summary.code, ErrorCode::INVALID_ARGUMENT);

        // Deletion needs the file gone first; a present file refuses.
        let v = version_of(&registry, &repo_id);
        let err = core_mark_resolved(
            &runner,
            &mut registry,
            &repo_id,
            v,
            &list.files[0].path_id,
            &preview.working_fingerprint,
            MarkResolution::Deletion,
        )
        .await
        .expect_err("present file must refuse deletion");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        std::fs::remove_file(repo.join("gone.txt")).expect("remove");
        let v = version_of(&registry, &repo_id);
        core_mark_resolved(
            &runner,
            &mut registry,
            &repo_id,
            v,
            &list.files[0].path_id,
            &preview.working_fingerprint,
            MarkResolution::Deletion,
        )
        .await
        .expect("deletion resolves");
        let empty = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert!(empty.files.is_empty());

        let v = version_of(&registry, &repo_id);
        let done = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: true,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect("complete");
        assert_eq!(parents_of(&repo), 2);
        assert!(!repo.join("gone.txt").exists());
        let _ = done;
    }

    #[tokio::test]
    async fn binary_and_symlink_conflicts_stay_external() {
        let (_dir, repo) = temp_repo("binsym");
        std::fs::write(repo.join("bin.dat"), b"\x00\x01base\n").expect("write");
        git(&repo, &["add", "bin.dat"]);
        git(&repo, &["commit", "-m", "add bin"]);
        #[cfg(unix)]
        std::os::unix::fs::symlink("base.txt", repo.join("link")).expect("symlink");
        git(&repo, &["add", "link"]);
        git(&repo, &["commit", "-m", "add link"]);

        git(&repo, &["checkout", "-b", "feature"]);
        std::fs::write(repo.join("bin.dat"), b"\x00\x01feature\n").expect("write");
        git(&repo, &["add", "bin.dat"]);
        std::fs::remove_file(repo.join("link")).expect("remove");
        #[cfg(unix)]
        std::os::unix::fs::symlink("feat.txt", repo.join("link")).expect("symlink");
        git(&repo, &["add", "link"]);
        std::fs::write(repo.join("feat.txt"), "feat\n").expect("write");
        git(&repo, &["add", "feat.txt"]);
        git(&repo, &["commit", "-m", "feature sides"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("bin.dat"), b"\x00\x01main\n").expect("write");
        git(&repo, &["add", "bin.dat"]);
        std::fs::remove_file(repo.join("link")).expect("remove");
        #[cfg(unix)]
        std::os::unix::fs::symlink("main.txt", repo.join("link")).expect("symlink");
        git(&repo, &["add", "link"]);
        std::fs::write(repo.join("main.txt"), "main\n").expect("write");
        git(&repo, &["add", "main.txt"]);
        git(&repo, &["commit", "-m", "main sides"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(started.conflicted);

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        let kinds: Vec<&str> = list.files.iter().map(|f| f.kind.as_str()).collect();
        assert!(kinds.contains(&"binary"), "kinds: {kinds:?}");
        assert!(kinds.contains(&"symlink"), "kinds: {kinds:?}");
        for file in &list.files {
            if file.kind == "text" {
                continue;
            }
            assert!(!file.supported);
            assert!(file.support_reason.as_ref().unwrap().contains("external"));
            let preview = core_preview(&runner, &registry, &repo_id, &file.path_id)
                .await
                .expect("preview");
            assert!(preview.supported_actions.is_empty());
            let v = version_of(&registry, &repo_id);
            let token = confirm_token(
                &mut registry,
                &repo_id,
                v,
                "conflict_accept",
                vec![file.path_id.clone(), "current".to_string()],
            );
            let err = core_accept(
                &runner,
                &mut registry,
                &repo_id,
                &ConflictAcceptRequest {
                    request_id: "test".to_string(),
                    repo_id: String::new(),
                    expected_version: v,
                    path_id: file.path_id.to_string(),
                    side: ("current").to_string(),
                    working_fingerprint: preview.working_fingerprint.to_string(),
                    confirmation_token: token.to_string(),
                },
            )
            .await
            .expect_err("unsupported accept must refuse");
            assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
        }
    }

    #[test]
    fn submodule_modes_classify_as_unsupported_with_external_reason() {
        // FX-10: submodule collisions never reach a writer; the reason
        // must point at an external tool.
        let entry = UnmergedEntry {
            path: b"sub".to_vec(),
            stages: [
                None,
                Some(("160000".to_string(), "1".repeat(40))),
                Some(("160000".to_string(), "2".repeat(40))),
                Some(("160000".to_string(), "3".repeat(40))),
            ],
        };
        let (kind, supported, reason) = kind_of(&entry, &[None, None, None]);
        assert_eq!(kind, "submodule");
        assert!(!supported);
        assert!(reason.contains("external"), "reason: {reason}");
    }

    #[tokio::test]
    async fn rename_rename_conflict_lists_entries_without_app_writes() {
        // FX-10: both sides rename orig.txt to different targets
        // (rename/rename). Listing must surface entries; the app writes
        // nothing by itself.
        let (_dir, repo) = temp_repo("rename");
        std::fs::write(repo.join("orig.txt"), "shared body\n").expect("write");
        git(&repo, &["add", "orig.txt"]);
        git(&repo, &["commit", "-m", "add orig"]);
        git(&repo, &["checkout", "-b", "side-a"]);
        git(&repo, &["mv", "orig.txt", "a.txt"]);
        git(&repo, &["commit", "-m", "rename to a"]);
        git(&repo, &["checkout", "main"]);
        git(&repo, &["checkout", "-b", "side-b"]);
        git(&repo, &["rm", "orig.txt"]);
        std::fs::write(repo.join("b.txt"), "shared body\n").expect("write");
        git(&repo, &["add", "b.txt"]);
        git(&repo, &["commit", "-m", "rename to b"]);
        git(&repo, &["checkout", "side-a"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/side-b",
            &target,
        )
        .await;
        assert!(started.conflicted, "rename/rename must conflict");

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert!(
            !list.files.is_empty(),
            "rename/rename must surface conflict entries"
        );
    }

    #[tokio::test]
    async fn abort_restores_premerge_head_behind_token() {
        let (_dir, repo) = temp_repo("abort");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        diverge_content(&repo);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(repo.join(".git").join("MERGE_HEAD").exists());

        // The abort summary names both sides and the recorded HEAD.
        let summary = abort_summary(&runner, &registry, &repo_id)
            .await
            .expect("summary");
        assert!(summary.contains("feature") && summary.contains("main"));

        let v = version_of(&registry, &repo_id);
        let token = confirm_token(
            &mut registry,
            &repo_id,
            v,
            "merge_abort",
            vec!["merge".to_string()],
        );
        let snap = core_abort(&runner, &mut registry, &repo_id, v, &token)
            .await
            .expect("abort");
        assert_eq!(head_of(&repo), target);
        assert_eq!(snap.state, RepoState::Normal);
        assert!(registry.merge_get(&repo_id).is_none());
        assert!(!repo.join(".git").join("MERGE_HEAD").exists());
        assert_eq!(
            std::fs::read(repo.join("f.txt")).expect("read"),
            b"base\nmain\n"
        );
    }

    #[tokio::test]
    async fn external_merge_abort_refuses_but_complete_works() {
        let (_dir, repo) = temp_repo("external");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        diverge_content(&repo);
        // A merge begun outside the app: MERGE_HEAD without a registry record.
        // Conflicts make `git merge` exit nonzero; that is the setup.
        let status = StdCommand::new("git")
            .current_dir(&repo)
            .args(["merge", "--no-ff", "--no-commit", "--no-edit", "feature"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .expect("spawn git");
        assert!(!status.success(), "fixture merge should conflict");
        assert!(repo.join(".git").join("MERGE_HEAD").exists());

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        assert!(registry.merge_get(&repo_id).is_none());

        let err = abort_summary(&runner, &registry, &repo_id)
            .await
            .expect_err("external abort summary must refuse");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert!(!list.can_abort);
        assert!(list.abort_reason.as_ref().unwrap().contains("terminal"));

        // External resolution still works: accept, stage, complete.
        let file = &list.files[0];
        let preview = core_preview(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("preview");
        assert_eq!(preview.current_label, "repo");
        assert_eq!(preview.incoming_label, "incoming");
        let v = version_of(&registry, &repo_id);
        let token = confirm_token(
            &mut registry,
            &repo_id,
            v,
            "conflict_accept",
            vec![file.path_id.clone(), "incoming".to_string()],
        );
        let accepted = core_accept(
            &runner,
            &mut registry,
            &repo_id,
            &ConflictAcceptRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                path_id: file.path_id.to_string(),
                side: ("incoming").to_string(),
                working_fingerprint: preview.working_fingerprint.to_string(),
                confirmation_token: token.to_string(),
            },
        )
        .await
        .expect("accept");
        live_status(&runner, &mut registry, &repo_id).await;
        let relist = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        let v = version_of(&registry, &repo_id);
        core_mark_resolved(
            &runner,
            &mut registry,
            &repo_id,
            v,
            &relist.files[0].path_id,
            &accepted.working_fingerprint,
            MarkResolution::WorkingFile,
        )
        .await
        .expect("mark resolved");
        let target = head_of(&repo);
        let v = version_of(&registry, &repo_id);
        let done = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: true,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect("external complete works");
        assert_eq!(parents_of(&repo), 2);
        let _ = done;
    }

    #[tokio::test]
    async fn complete_with_unmerged_entries_refuses() {
        let (_dir, repo) = temp_repo("blocked");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        diverge_content(&repo);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;

        let v = version_of(&registry, &repo_id);
        let err = core_complete(
            &runner,
            &mut registry,
            &repo_id,
            &MergeCompleteRequest {
                request_id: "test".to_string(),
                repo_id: String::new(),
                expected_version: v,
                subject: ("Merge feature").to_string(),
                body: ("").to_string(),
                staged_reviewed: true,
                confirmed_head_oid: target.to_string(),
            },
        )
        .await
        .expect_err("unmerged must block complete");
        assert_eq!(err.code, ErrorCode::CONFLICTS_PRESENT);
        // The merge is still open afterwards.
        assert!(repo.join(".git").join("MERGE_HEAD").exists());
    }

    fn block_of(segments: &[MergeSegment]) -> &MergeBlock {
        segments
            .iter()
            .find_map(|s| match s {
                MergeSegment::Conflict(b) => Some(b),
                _ => None,
            })
            .expect("one conflict block")
    }

    #[test]
    fn merge_dto_json_matches_frontend_contract() {
        let hunks = ConflictHunks {
            path_id: "f.txt".to_string(),
            display_path: "f.txt".to_string(),
            current_label: "main".to_string(),
            incoming_label: "feature".to_string(),
            working_fingerprint: "ab:1".to_string(),
            working_text: "top\nraw".to_string(),
            segments: vec![
                MergeSegmentDto::Clean {
                    lines: vec!["top".to_string()],
                },
                MergeSegmentDto::Conflict {
                    hunk_id: "h1".to_string(),
                    current: vec!["c".to_string()],
                    incoming: vec!["i".to_string()],
                    base: vec![],
                    raw: "raw".to_string(),
                },
            ],
            conflict_count: 1,
        };
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&hunks).expect("json")).expect("parse");
        assert_eq!(value["workingFingerprint"], "ab:1");
        assert_eq!(value["workingText"], "top\nraw");
        assert_eq!(value["conflictCount"], 1);
        assert_eq!(value["segments"][0]["kind"], "clean");
        assert_eq!(value["segments"][1]["kind"], "conflict");
        assert_eq!(value["segments"][1]["hunkId"], "h1");
        let pick: MergeBlockPick =
            serde_json::from_str(r#"{"hunkId":"h1","lines":[{"side":"incoming","index":2}]}"#)
                .expect("pick");
        assert_eq!(pick.lines[0].side, "incoming");
        assert_eq!(pick.lines[0].index, 2);
        let result = ConflictAutoResolveResult {
            working_fingerprint: "fp".into(),
            picks: vec![pick],
        };
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["workingFingerprint"], "fp");
        assert_eq!(json["picks"][0]["hunkId"], "h1");
    }

    #[test]
    fn merge_parser_splits_clean_and_conflict() {
        let bytes = b"head\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\ntail\n";
        let segments = parse_merge_file(bytes).expect("parse");
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], MergeSegment::Clean { bytes } if bytes == b"head\n"));
        assert!(matches!(&segments[2], MergeSegment::Clean { bytes } if bytes == b"tail\n"));
        let block = block_of(&segments);
        assert_eq!(block.current, vec![b"ours\n".to_vec()]);
        assert_eq!(block.incoming, vec![b"theirs\n".to_vec()]);
        assert!(block.base.is_empty());
        assert!(!block.id.is_empty());
        // Identity is stable for identical bytes.
        let again = parse_merge_file(bytes).expect("parse");
        assert_eq!(block_of(&again).id, block.id);
    }

    #[test]
    fn merge_parser_reads_diff3_base() {
        let bytes = b"<<<<<<< HEAD\nours\n||||||| base\norig\n=======\ntheirs\n>>>>>>> feature\n";
        let segments = parse_merge_file(bytes).expect("parse");
        let block = block_of(&segments);
        assert_eq!(block.current, vec![b"ours\n".to_vec()]);
        assert_eq!(block.base, vec![b"orig\n".to_vec()]);
        assert_eq!(block.incoming, vec![b"theirs\n".to_vec()]);
    }

    #[test]
    fn merge_parser_rejects_malformed() {
        // Unclosed block at EOF.
        assert!(parse_merge_file(b"a\n<<<<<<< HEAD\nx\n=======\ny\n").is_err());
        // Nested opener inside a block.
        assert!(
            parse_merge_file(b"<<<<<<< HEAD\nx\n<<<<<<< HEAD\ny\n=======\nz\n>>>>>>> f\n").is_err()
        );
        // Closer without a separator.
        assert!(parse_merge_file(b"<<<<<<< HEAD\nx\n>>>>>>> f\n").is_err());
        // Separator-looking lines outside any block stay clean content.
        let segments = parse_merge_file(b"=======\nplain\n>>>>>>> nope\n").expect("clean");
        assert_eq!(segments.len(), 1);
        assert!(matches!(&segments[0], MergeSegment::Clean { .. }));
    }

    #[test]
    fn merge_parser_preserves_endings_byte_exact() {
        let bytes = b"a\r\n<<<<<<< HEAD\nx\r\n=======\r\ny\r\n>>>>>>> f\r\ntail";
        let segments = parse_merge_file(bytes).expect("parse");
        let block = block_of(&segments);
        assert_eq!(block.current, vec![b"x\r\n".to_vec()]);
        assert_eq!(block.incoming, vec![b"y\r\n".to_vec()]);
        assert!(matches!(
            segment_dto(&segments[0]),
            MergeSegmentDto::Clean { ref lines } if lines == &["a".to_string()]
        ));
        // Round trip: clean bytes plus the verbatim block reproduce the file.
        let mut rebuilt = Vec::new();
        for segment in &segments {
            match segment {
                MergeSegment::Clean { bytes } => rebuilt.extend_from_slice(bytes),
                MergeSegment::Conflict(b) => rebuilt.extend_from_slice(&b.raw),
            }
        }
        assert_eq!(rebuilt, bytes);
    }

    /// Two-line-per-side conflicts in two blocks, kept apart by enough
    /// context that git does not fuse them into one marker block.
    const PAD: &str = "p1\np2\np3\np4\np5\n";

    fn diverge_two_blocks(repo: &std::path::Path) {
        git(repo, &["checkout", "-b", "feature"]);
        std::fs::write(
            repo.join("f.txt"),
            format!("top\nF1\nF2\n{PAD}mid\nG1\nG2\nbottom\n"),
        )
        .expect("write");
        git(repo, &["add", "f.txt"]);
        git(repo, &["commit", "-m", "feature side"]);
        git(repo, &["checkout", "main"]);
        std::fs::write(
            repo.join("f.txt"),
            format!("top\nC1\nC2\n{PAD}mid\nH1\nH2\nbottom\n"),
        )
        .expect("write");
        git(repo, &["add", "f.txt"]);
        git(repo, &["commit", "-m", "main side"]);
    }

    async fn conflicted_repo(
        label: &str,
        base: String,
    ) -> (
        std::path::PathBuf,
        std::path::PathBuf,
        GitRunner,
        RepoRegistry,
        String,
        ConflictFile,
    ) {
        let (_dir, repo) = temp_repo(label);
        std::fs::write(repo.join("f.txt"), base).expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        diverge_two_blocks(&repo);
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let target = head_of(&repo);
        let started = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &target,
        )
        .await;
        assert!(started.conflicted);
        live_status(&runner, &mut registry, &repo_id).await;
        let list = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(list.files.len(), 1);
        let file = list.files[0].clone();
        assert_eq!(file.kind, "text");
        (_dir, repo, runner, registry, repo_id, file)
    }

    fn merge_request(
        repo_id: &str,
        version: u64,
        path_id: &str,
        fingerprint: &str,
        picks: Vec<MergeBlockPick>,
    ) -> ConflictMergeRequest {
        ConflictMergeRequest {
            request_id: "test".to_string(),
            repo_id: repo_id.to_string(),
            expected_version: version,
            path_id: path_id.to_string(),
            working_fingerprint: fingerprint.to_string(),
            picks,
            result_text: None,
        }
    }

    fn take_all(hunk_id: &str, side: &str, count: usize) -> MergeBlockPick {
        MergeBlockPick {
            hunk_id: hunk_id.to_string(),
            lines: (0..count)
                .map(|index| MergePickedLine {
                    side: side.to_string(),
                    index,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn merge_hunks_lists_blocks_from_live_conflict() {
        let (_d, _r, runner, registry, repo_id, file) = conflicted_repo(
            "merge-hunks",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        let hunks = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("hunks");
        assert_eq!(hunks.conflict_count, 2);
        assert_eq!(hunks.current_label, "main");
        assert_eq!(hunks.incoming_label, "feature");
        let conflicts: Vec<_> = hunks
            .segments
            .iter()
            .filter_map(|s| match s {
                MergeSegmentDto::Conflict {
                    hunk_id,
                    current,
                    incoming,
                    ..
                } => Some((hunk_id.clone(), current.clone(), incoming.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].1, vec!["C1".to_string(), "C2".to_string()]);
        assert_eq!(conflicts[0].2, vec!["F1".to_string(), "F2".to_string()]);
        assert_ne!(conflicts[0].0, conflicts[1].0);
    }

    #[tokio::test]
    async fn merge_resolve_takes_whole_block_and_mixes_lines() {
        let (_d, repo, runner, mut registry, repo_id, file) =
            conflicted_repo("merge-mix", format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n")).await;
        let hunks = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("hunks");
        let ids: Vec<String> = hunks
            .segments
            .iter()
            .filter_map(|s| match s {
                MergeSegmentDto::Conflict { hunk_id, .. } => Some(hunk_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 2);
        // Block one takes current whole; block two mixes current line 0
        // with incoming line 1.
        let v = version_of(&registry, &repo_id);
        let result = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                &hunks.working_fingerprint,
                vec![
                    take_all(&ids[0], "current", 2),
                    MergeBlockPick {
                        hunk_id: ids[1].clone(),
                        lines: vec![
                            MergePickedLine {
                                side: "current".to_string(),
                                index: 0,
                            },
                            MergePickedLine {
                                side: "incoming".to_string(),
                                index: 1,
                            },
                        ],
                    },
                ],
            ),
        )
        .await
        .expect("merge");
        assert_eq!(result.resolved_blocks, 2);
        assert_eq!(result.remaining_blocks, 0);
        assert_eq!(
            std::fs::read_to_string(repo.join("f.txt")).expect("read"),
            format!("top\nC1\nC2\n{PAD}mid\nH1\nG2\nbottom\n")
        );
        assert_eq!(
            result.working_fingerprint,
            fingerprint(&std::fs::read(repo.join("f.txt")).expect("read"))
        );
    }

    #[tokio::test]
    async fn merge_resolve_keeps_unpicked_markers() {
        let (_d, repo, runner, mut registry, repo_id, file) = conflicted_repo(
            "merge-partial",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        let hunks = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("hunks");
        let first = hunks
            .segments
            .iter()
            .find_map(|s| match s {
                MergeSegmentDto::Conflict { hunk_id, .. } => Some(hunk_id.clone()),
                _ => None,
            })
            .expect("first block");
        let v = version_of(&registry, &repo_id);
        let result = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                &hunks.working_fingerprint,
                vec![take_all(&first, "incoming", 2)],
            ),
        )
        .await
        .expect("partial merge");
        assert_eq!(result.resolved_blocks, 1);
        assert_eq!(result.remaining_blocks, 1);
        let text = std::fs::read_to_string(repo.join("f.txt")).expect("read");
        assert!(text.starts_with(&format!("top\nF1\nF2\n{PAD}mid\n")));
        assert!(text.contains("<<<<<<<"));
        assert!(text.contains(">>>>>>>"));
    }

    #[tokio::test]
    async fn merge_resolve_accepts_explicit_empty_block_selection() {
        let (_d, repo, runner, mut registry, repo_id, file) = conflicted_repo(
            "merge-empty",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        // A text conflict may have no lines on one side (a deleted region).
        let working = "top\r\n<<<<<<< HEAD\r\n=======\r\nincoming\r\n>>>>>>> feature\r\ntail";
        std::fs::write(repo.join("f.txt"), working).expect("write empty-side markers");
        let hunks = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("hunks");
        let hunk_id = hunks
            .segments
            .iter()
            .find_map(|segment| match segment {
                MergeSegmentDto::Conflict {
                    hunk_id, current, ..
                } => {
                    assert!(current.is_empty());
                    Some(hunk_id.clone())
                }
                _ => None,
            })
            .expect("empty-side conflict");
        let version = version_of(&registry, &repo_id);
        let result = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                version,
                &file.path_id,
                &hunks.working_fingerprint,
                vec![MergeBlockPick {
                    hunk_id,
                    lines: vec![],
                }],
            ),
        )
        .await
        .expect("resolve with empty side");
        assert_eq!(result.resolved_blocks, 1);
        assert_eq!(result.remaining_blocks, 0);
        assert_eq!(
            std::fs::read(repo.join("f.txt")).expect("read"),
            b"top\r\ntail"
        );
        let listed = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("unmerged index");
        assert_eq!(listed.files.len(), 1, "resolution must stay unstaged");
    }

    #[test]
    fn auto_base_lookup_trims_only_unchanged_edges_and_rejects_ambiguous_bases() {
        let canonical = b"<<<<<<< current\nsame\nA\nb\ntail\n||||||| base\nsame\na\nb\ntail\n=======\nsame\na\nB\ntail\n>>>>>>> incoming\n";
        let bases = index_block_bases(parse_merge_file(canonical).unwrap());
        let key = (
            vec![b"A\n".to_vec(), b"b\n".to_vec()],
            vec![b"a\n".to_vec(), b"B\n".to_vec()],
        );
        assert_eq!(
            bases.get(&key),
            Some(&Some(vec![b"a\n".to_vec(), b"b\n".to_vec()]))
        );
        let ambiguous = b"<<<<<<< current\nours\n||||||| base\nbase1\n=======\ntheirs\n>>>>>>> incoming\n<<<<<<< current\nours\n||||||| base\nbase2\n=======\ntheirs\n>>>>>>> incoming\n";
        let bases = index_block_bases(parse_merge_file(ambiguous).unwrap());
        assert_eq!(
            bases.get(&(vec![b"ours\n".to_vec()], vec![b"theirs\n".to_vec()])),
            Some(&None)
        );
        let changed_edge = b"<<<<<<< current\nshared addition\nours\n||||||| base\nold\n=======\nshared addition\ntheirs\n>>>>>>> incoming\n";
        let bases = index_block_bases(parse_merge_file(changed_edge).unwrap());
        assert!(!bases.contains_key(&(vec![b"ours\n".to_vec()], vec![b"theirs\n".to_vec()])));
    }

    #[tokio::test]
    async fn auto_resolve_recovers_base_preserves_working_edits_and_leaves_overlap() {
        let (_dir, repo) = temp_repo("auto-adjacent");
        std::fs::write(repo.join("f.txt"), format!("top\na\nb\n{PAD}old\ntail\n")).unwrap();
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        git(&repo, &["checkout", "-b", "feature"]);
        std::fs::write(
            repo.join("f.txt"),
            format!("top\na\nB\n{PAD}incoming\ntail\n"),
        )
        .unwrap();
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "incoming"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(
            repo.join("f.txt"),
            format!("top\nA\nb\n{PAD}current\ntail\n"),
        )
        .unwrap();
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "current"]);
        let runner = git_runner().unwrap();
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let result = start(
            &runner,
            &mut registry,
            &repo_id,
            "refs/heads/feature",
            &head_of(&repo),
        )
        .await;
        assert!(result.conflicted);
        live_status(&runner, &mut registry, &repo_id).await;
        let files = core_list(&runner, &mut registry, &repo_id)
            .await
            .unwrap()
            .files;
        let path_id = &files[0].path_id;
        let working = std::fs::read_to_string(repo.join("f.txt"))
            .unwrap()
            .replacen("top\n", "edited context\n", 1);
        std::fs::write(repo.join("f.txt"), &working).unwrap();
        let doc = core_hunks(&runner, &registry, &repo_id, path_id)
            .await
            .unwrap();
        assert_eq!(doc.conflict_count, 2);
        let index = std::fs::read(repo.join(".git/index")).unwrap();
        let version = version_of(&registry, &repo_id);
        let request = ConflictAutoResolveRequest {
            request_id: "auto-test".into(),
            repo_id: repo_id.clone(),
            expected_version: version,
            path_id: path_id.clone(),
            working_fingerprint: doc.working_fingerprint.clone(),
        };
        let auto = core_auto_resolve(&runner, &mut registry, &request)
            .await
            .unwrap();
        assert_eq!(auto.picks.len(), 1);
        assert_eq!(auto.picks[0].lines.len(), 2);
        assert_eq!(
            std::fs::read_to_string(repo.join("f.txt")).unwrap(),
            working
        );
        assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
        assert_eq!(version_of(&registry, &repo_id), version);
        let save = merge_request(
            &repo_id,
            version,
            path_id,
            &doc.working_fingerprint,
            auto.picks,
        );
        let saved = core_merge(&runner, &mut registry, &repo_id, &save)
            .await
            .unwrap();
        assert_eq!(saved.remaining_blocks, 1);
        let after = std::fs::read_to_string(repo.join("f.txt")).unwrap();
        assert!(after.starts_with("edited context\nA\nB\n"));
        assert!(after.contains("<<<<<<< HEAD\ncurrent\n=======\nincoming\n"));
        assert_eq!(std::fs::read(repo.join(".git/index")).unwrap(), index);
    }

    #[tokio::test]
    async fn auto_resolve_rejects_stale_requests_and_preserves_ambiguous_file() {
        let (_dir, repo, runner, mut registry, repo_id, file) =
            conflicted_repo("auto-stale", format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n")).await;
        let doc = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .unwrap();
        let before = std::fs::read(repo.join("f.txt")).unwrap();
        let mut request = ConflictAutoResolveRequest {
            request_id: "auto-test".into(),
            repo_id: repo_id.clone(),
            expected_version: version_of(&registry, &repo_id),
            path_id: file.path_id,
            working_fingerprint: "stale".into(),
        };
        assert_eq!(
            core_auto_resolve(&runner, &mut registry, &request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::STALE_STATE
        );
        request.working_fingerprint = doc.working_fingerprint;
        assert!(core_auto_resolve(&runner, &mut registry, &request)
            .await
            .unwrap()
            .picks
            .is_empty());
        request.expected_version += 1;
        assert_eq!(
            core_auto_resolve(&runner, &mut registry, &request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::STALE_STATE
        );
        assert_eq!(std::fs::read(repo.join("f.txt")).unwrap(), before);
    }

    #[tokio::test]
    async fn auto_resolve_handles_diff3_deletions_empty_base_and_identical_sides() {
        let (_dir, repo, runner, mut registry, repo_id, file) =
            conflicted_repo("auto-diff3", format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n")).await;
        let text = "prefix\r\n<<<<<<< HEAD\r\nold\r\n||||||| base\r\nold\r\n=======\r\n>>>>>>> feature\r\n<<<<<<< HEAD\r\n||||||| base\r\n=======\r\nadded\r\n>>>>>>> feature\r\n<<<<<<< HEAD\r\nsame\r\n=======\r\nsame\r\n>>>>>>> feature\r\n<<<<<<< HEAD\r\n=======\r\nunknown base\r\n>>>>>>> feature\r\ntail";
        std::fs::write(repo.join("f.txt"), text).unwrap();
        let doc = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .unwrap();
        let request = ConflictAutoResolveRequest {
            request_id: "auto-test".into(),
            repo_id: repo_id.clone(),
            expected_version: version_of(&registry, &repo_id),
            path_id: file.path_id.clone(),
            working_fingerprint: doc.working_fingerprint.clone(),
        };
        let auto = core_auto_resolve(&runner, &mut registry, &request)
            .await
            .unwrap();
        assert_eq!(auto.picks.len(), 3);
        assert!(auto.picks[0].lines.is_empty());
        let save = merge_request(
            &repo_id,
            request.expected_version,
            &file.path_id,
            &doc.working_fingerprint,
            auto.picks,
        );
        let saved = core_merge(&runner, &mut registry, &repo_id, &save)
            .await
            .unwrap();
        assert_eq!(saved.remaining_blocks, 1);
        assert_eq!(std::fs::read_to_string(repo.join("f.txt")).unwrap(), "prefix\r\nadded\r\nsame\r\n<<<<<<< HEAD\r\n=======\r\nunknown base\r\n>>>>>>> feature\r\ntail");
    }

    #[tokio::test]
    async fn edited_result_saves_full_text_preserving_mode_and_unmerged_index() {
        let (_d, repo, runner, mut registry, repo_id, file) = conflicted_repo(
            "edited-result",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(repo.join("f.txt"), std::fs::Permissions::from_mode(0o755))
                .expect("mode");
        }
        let doc = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("doc");
        assert!(doc.working_text.starts_with("top\n"));
        let mut request = merge_request(
            &repo_id,
            version_of(&registry, &repo_id),
            &file.path_id,
            &doc.working_fingerprint,
            vec![],
        );
        request.result_text =
            Some("edited clean context\r\ncustom resolution\r\ntail without newline".into());
        let saved = core_merge(&runner, &mut registry, &repo_id, &request)
            .await
            .expect("save");
        assert_eq!(saved.remaining_blocks, 0);
        assert_eq!(saved.resolved_blocks, 2);
        assert_eq!(
            std::fs::read_to_string(repo.join("f.txt")).expect("read"),
            request.result_text.clone().unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(repo.join("f.txt"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
        live_status(&runner, &mut registry, &repo_id).await;
        let files = core_list(&runner, &mut registry, &repo_id)
            .await
            .unwrap()
            .files;
        assert_eq!(files.len(), 1, "saving must keep the index unmerged");
        let path_id = &files[0].path_id;
        let reopened = core_hunks(&runner, &registry, &repo_id, path_id)
            .await
            .expect("reopen resolved text");
        assert_eq!(reopened.conflict_count, 0);
        let mut clear = merge_request(
            &repo_id,
            version_of(&registry, &repo_id),
            path_id,
            &saved.working_fingerprint,
            vec![],
        );
        clear.result_text = Some(String::new());
        core_merge(&runner, &mut registry, &repo_id, &clear)
            .await
            .expect("empty result");
        assert_eq!(std::fs::read(repo.join("f.txt")).unwrap(), b"");
    }

    #[tokio::test]
    async fn edited_result_rejects_stale_invalid_and_mixed_requests_without_writing() {
        let (_d, repo, runner, mut registry, repo_id, file) = conflicted_repo(
            "edited-invalid",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        let doc = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .unwrap();
        let before = std::fs::read(repo.join("f.txt")).unwrap();
        for invalid in [
            "bad\0text".to_string(),
            "x".repeat(MERGE_CAP_BYTES + 1),
            "<<<<<<< HEAD\nunfinished\n".to_string(),
        ] {
            let mut request = merge_request(
                &repo_id,
                version_of(&registry, &repo_id),
                &file.path_id,
                &doc.working_fingerprint,
                vec![],
            );
            request.result_text = Some(invalid);
            assert!(core_merge(&runner, &mut registry, &repo_id, &request)
                .await
                .is_err());
            assert_eq!(std::fs::read(repo.join("f.txt")).unwrap(), before);
        }
        let mut request = merge_request(
            &repo_id,
            version_of(&registry, &repo_id),
            &file.path_id,
            "outdated",
            vec![],
        );
        request.result_text = Some("new text".into());
        assert_eq!(
            core_merge(&runner, &mut registry, &repo_id, &request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::STALE_STATE
        );
        request.working_fingerprint = doc.working_fingerprint;
        request.picks = vec![take_all("any", "current", 1)];
        assert_eq!(
            core_merge(&runner, &mut registry, &repo_id, &request)
                .await
                .unwrap_err()
                .code,
            ErrorCode::INVALID_ARGUMENT
        );
        assert_eq!(std::fs::read(repo.join("f.txt")).unwrap(), before);
    }

    #[tokio::test]
    async fn edited_partial_result_can_be_reopened() {
        let (_d, repo, runner, mut registry, repo_id, file) = conflicted_repo(
            "edited-partial",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        let doc = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .unwrap();
        let mut request = merge_request(
            &repo_id,
            version_of(&registry, &repo_id),
            &file.path_id,
            &doc.working_fingerprint,
            vec![],
        );
        let edited =
            "custom prefix\n<<<<<<< HEAD\none\n=======\ntwo\n>>>>>>> feature\ncustom suffix\n";
        request.result_text = Some(edited.into());
        let saved = core_merge(&runner, &mut registry, &repo_id, &request)
            .await
            .unwrap();
        assert_eq!(saved.remaining_blocks, 1);
        assert_eq!(saved.resolved_blocks, 1);
        assert_eq!(std::fs::read_to_string(repo.join("f.txt")).unwrap(), edited);
        live_status(&runner, &mut registry, &repo_id).await;
        let files = core_list(&runner, &mut registry, &repo_id)
            .await
            .unwrap()
            .files;
        let reopened = core_hunks(&runner, &registry, &repo_id, &files[0].path_id)
            .await
            .unwrap();
        assert_eq!(reopened.conflict_count, 1);
        assert_eq!(reopened.working_text, edited);
    }

    #[tokio::test]
    async fn merge_resolve_rejects_stale_and_invalid_picks() {
        let (_d, _r, runner, mut registry, repo_id, file) = conflicted_repo(
            "merge-reject",
            format!("top\nX\nY\n{PAD}mid\nZ\nW\nbottom\n"),
        )
        .await;
        let hunks = core_hunks(&runner, &registry, &repo_id, &file.path_id)
            .await
            .expect("hunks");
        let first = hunks
            .segments
            .iter()
            .find_map(|s| match s {
                MergeSegmentDto::Conflict { hunk_id, .. } => Some(hunk_id.clone()),
                _ => None,
            })
            .expect("first block");

        // Stale fingerprint fails closed without touching the file.
        let before = hunks.working_fingerprint.clone();
        let v = version_of(&registry, &repo_id);
        let err = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                "deadbeef:1",
                vec![take_all(&first, "current", 2)],
            ),
        )
        .await
        .expect_err("stale fingerprint");
        assert_eq!(err.code, ErrorCode::STALE_STATE);

        // Unknown hunk and out-of-range index mean drift.
        let v = version_of(&registry, &repo_id);
        let err = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                &before,
                vec![take_all("nope", "current", 1)],
            ),
        )
        .await
        .expect_err("unknown hunk");
        assert_eq!(err.code, ErrorCode::STALE_STATE);
        let v = version_of(&registry, &repo_id);
        let err = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                &before,
                vec![MergeBlockPick {
                    hunk_id: first.clone(),
                    lines: vec![MergePickedLine {
                        side: "current".to_string(),
                        index: 99,
                    }],
                }],
            ),
        )
        .await
        .expect_err("out of range");
        assert_eq!(err.code, ErrorCode::STALE_STATE);

        // Malformed requests fail as invalid arguments instead.
        let v = version_of(&registry, &repo_id);
        let err = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(&repo_id, v, &file.path_id, &before, vec![]),
        )
        .await
        .expect_err("empty picks");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
        let v = version_of(&registry, &repo_id);
        let err = core_merge(
            &runner,
            &mut registry,
            &repo_id,
            &merge_request(
                &repo_id,
                v,
                &file.path_id,
                &before,
                vec![
                    take_all(&first, "current", 2),
                    take_all(&first, "incoming", 2),
                ],
            ),
        )
        .await
        .expect_err("duplicate hunk");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn conflict_list_tracks_resolved_files_and_bulk_mark_stages_all() {
        let (_dir, repo, runner, mut registry, repo_id, _file) = conflicted_repo(
            "resolved-list",
            format!("top\nA\nB\n{PAD}mid\nC\nD\nbottom\n"),
        )
        .await;
        std::fs::write(repo.join("already-resolved.txt"), "resolved\n").expect("write");
        git(&repo, &["add", "already-resolved.txt"]);

        let listed = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("list");
        assert_eq!(listed.current_label, "main");
        assert_eq!(listed.incoming_label, "feature");
        assert!(listed
            .resolved_files
            .iter()
            .any(|file| file.display_path == "already-resolved.txt" && file.status == "A"));

        let version = version_of(&registry, &repo_id);
        let snapshot = core_mark_all_resolved(&runner, &mut registry, &repo_id, version)
            .await
            .expect("mark all resolved");
        assert_eq!(snapshot.state, RepoState::Merging);

        let relisted = core_list(&runner, &mut registry, &repo_id)
            .await
            .expect("relist");
        assert!(relisted.files.is_empty());
        assert!(relisted.can_complete);
        assert!(relisted
            .resolved_files
            .iter()
            .any(|file| file.display_path == "f.txt" && file.status == "M"));
    }
}
