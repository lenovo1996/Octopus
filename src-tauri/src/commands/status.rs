//! Worktree status reads and the operation coordinator (T07).
//!
//! `repo_status` is trust-gated: read-only sessions get `TRUST_REQUIRED`
//! because Git may run content filters when comparing worktree to index.
//! Every fresh listing bumps the generation and caches raw byte paths, so
//! path ids handed to the UI always resolve against the listing that issued
//! them and older tokens fail with `STALE_STATE`.
//!
//! The coordinator keeps an in-memory job registry: request ids dedup double
//! submits, `operation_get` is the fallback when an event listener misses a
//! terminal update, and close is rejected while a job is still active.

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, ChangedFile, ErrorCode, OperationRecord, RecoveryAction, RequestId,
    StatusData, TrustState,
};
use crate::git::{read_status, to_display_rows, GitRunner, StatusError};
use crate::services::RepoRegistry;

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

fn trust_required() -> AppError {
    AppError::new(
        ErrorCode::TRUST_REQUIRED,
        "Trust this repository to read working changes",
        RecoveryAction::InspectState,
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

fn status_error(error: StatusError) -> AppError {
    match error {
        StatusError::Run(run) => match run {
            crate::git::runner::RunError::TimedOut => AppError::new(
                ErrorCode::TIMEOUT,
                "Reading working changes timed out",
                RecoveryAction::RetryRead,
                true,
            ),
            crate::git::runner::RunError::OutputLimit => AppError::new(
                ErrorCode::OUTPUT_LIMIT,
                "Working changes output exceeded the safety bound",
                RecoveryAction::RetryRead,
                true,
            ),
            crate::git::runner::RunError::SpawnFailed(_) => AppError::new(
                ErrorCode::GIT_ERROR,
                "Failed to read working changes",
                RecoveryAction::Refresh,
                true,
            ),
        },
        StatusError::GitFailed(_) => AppError::new(
            ErrorCode::GIT_ERROR,
            "Failed to read working changes",
            RecoveryAction::Refresh,
            true,
        ),
        StatusError::Malformed(_) => AppError::new(
            ErrorCode::GIT_ERROR,
            "Git returned an unreadable status listing",
            RecoveryAction::Refresh,
            true,
        ),
    }
}

async fn core_status(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
) -> Result<StatusData, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required());
    }
    let parsed = read_status(runner, &session.worktree_root)
        .await
        .map_err(status_error)?;
    let counts_match = parsed.files.len();
    let raw_paths: Vec<Vec<u8>> = parsed.files.iter().map(|f| f.path.clone()).collect();
    let raw_orig: Vec<Option<Vec<u8>>> = parsed.files.iter().map(|f| f.orig_path.clone()).collect();
    debug_assert_eq!(raw_paths.len(), counts_match);
    registry.status_put(repo_id, raw_paths, raw_orig);
    let rows = to_display_rows(&parsed);
    let files: Vec<ChangedFile> = rows
        .into_iter()
        .enumerate()
        .map(
            |(
                index,
                (
                    display_path,
                    original_display_path,
                    index_status,
                    worktree_status,
                    kind,
                    conflicted,
                ),
            )| {
                ChangedFile {
                    path_id: registry.status_path_id(repo_id, index).unwrap_or_default(),
                    display_path,
                    original_display_path,
                    index_status,
                    worktree_status,
                    kind,
                    conflicted,
                }
            },
        )
        .collect();
    Ok(StatusData { files })
}

/// Snapshot counts for a trusted session. Read failures degrade to null
/// counts (unavailable) rather than failing the whole snapshot; the
/// dedicated `repo_status` read reports the error itself.
pub async fn snapshot_counts(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    worktree_root: &std::path::Path,
    trusted: bool,
) -> (Option<u64>, Option<u64>, Option<u64>) {
    if !trusted {
        return (None, None, None);
    }
    let parsed = match read_status(runner, worktree_root).await {
        Ok(parsed) => parsed,
        Err(_) => return (None, None, None),
    };
    let counts = parsed.counts();
    // Cache raw paths alongside so path ids stay valid for this listing.
    let raw_paths: Vec<Vec<u8>> = parsed.files.iter().map(|f| f.path.clone()).collect();
    let raw_orig: Vec<Option<Vec<u8>>> = parsed.files.iter().map(|f| f.orig_path.clone()).collect();
    registry.status_put(repo_id, raw_paths, raw_orig);
    (
        Some(counts.staged as u64),
        Some(counts.unstaged as u64),
        Some(counts.conflicted as u64),
    )
}

fn core_operation_get(
    registry: &RepoRegistry,
    operation_id: &str,
) -> Result<OperationRecord, AppError> {
    registry.job_get(operation_id).cloned().ok_or_else(|| {
        AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            "Unknown operation; it may belong to a closed repository",
            RecoveryAction::InspectState,
            false,
        )
    })
}

fn core_operation_cancel(
    registry: &mut RepoRegistry,
    operation_id: &str,
) -> Result<bool, AppError> {
    // Real cancellation: the job task polls the flag and kills its child.
    // Unknown or terminal operations report false.
    Ok(registry.job_request_cancel(operation_id))
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoContext {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRequest {
    pub request_id: RequestId,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelResponse {
    pub cancel_requested: bool,
}

// ---- Commands ----

#[tauri::command]
pub async fn repo_status(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoContext,
) -> Result<ApiResult<StatusData>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_status(&runner, &mut registry, &request.repo_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn operation_get(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: OperationRequest,
) -> Result<ApiResult<OperationRecord>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let registry = registry.lock().await;
        core_operation_get(&registry, &request.operation_id)
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn operation_cancel(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: OperationRequest,
) -> Result<ApiResult<CancelResponse>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let mut registry = registry.lock().await;
        core_operation_cancel(&mut registry, &request.operation_id)
            .map(|cancel_requested| CancelResponse { cancel_requested })
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

pub mod prelude {
    pub use super::{operation_cancel, operation_get, repo_status, snapshot_counts};
    pub use super::{CancelResponse, OperationRequest, RepoContext};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::OperationState;
    use std::process::Command as StdCommand;

    fn temp_repo(label: &str) -> (tempfile_like::TempRoot, std::path::PathBuf) {
        let root = tempfile_like::TempRoot::new(label);
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        (root, repo)
    }

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

    /// Minimal temp-dir helper scoped to this module (no new dev-dependency).
    mod tempfile_like {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        pub struct TempRoot(std::path::PathBuf);

        impl TempRoot {
            pub fn new(label: &str) -> Self {
                let id = COUNTER.fetch_add(1, Ordering::SeqCst);
                let dir = std::env::temp_dir().join(format!(
                    "gitdock-t07-status-{}-{}-{label}",
                    std::process::id(),
                    id
                ));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).expect("temp root");
                Self(dir)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for TempRoot {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn open_session(
        registry: &mut RepoRegistry,
        repo: &std::path::Path,
        trust: TrustState,
    ) -> String {
        let discovered = crate::git::DiscoveredRepo {
            worktree_root: repo.to_path_buf(),
            git_dir: repo.join(".git"),
            common_dir: repo.join(".git"),
            object_format: "sha1".to_string(),
            bare: false,
        };
        registry.open(&discovered, trust).repo_id
    }

    fn test_record(request_id: &str, repo_id: Option<String>) -> OperationRecord {
        OperationRecord {
            operation_id: format!("op-{request_id}"),
            request_id: request_id.to_string(),
            repo_id,
            kind: "test.mutation".to_string(),
            state: OperationState::Running,
            stage: "working".to_string(),
            progress: None,
            error_code: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn status_is_trust_gated_and_lists_staged_unstaged() {
        let (_root, repo) = temp_repo("gated");
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_session(&mut registry, &repo, TrustState::ReadOnly);

        // Read-only sessions cannot read worktree status (content filters).
        let denied = core_status(&runner, &mut registry, &repo_id).await;
        assert!(matches!(
            denied,
            Err(ref e) if e.code == ErrorCode::TRUST_REQUIRED
        ));

        registry.get_mut(&repo_id).expect("session").trust = TrustState::Trusted;
        git(&repo, &["add", "a.txt"]);
        std::fs::write(repo.join("a.txt"), "a\nmore\n").expect("write");
        let data = core_status(&runner, &mut registry, &repo_id)
            .await
            .expect("status");
        assert_eq!(data.files.len(), 1);
        let file = &data.files[0];
        // Same file staged and unstaged stays one row with both flags.
        assert_eq!(file.index_status, "A");
        assert_eq!(file.worktree_status, "M");
        assert!(!file.path_id.is_empty());
        // Token resolves to the exact raw path of this listing.
        let (raw, orig) = registry
            .status_resolve(&repo_id, &file.path_id)
            .expect("resolve");
        assert_eq!(raw, b"a.txt");
        assert_eq!(orig, None);
    }

    #[tokio::test]
    async fn external_change_invalidates_previous_tokens() {
        let (_root, repo) = temp_repo("stale");
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_session(&mut registry, &repo, TrustState::Trusted);

        std::fs::write(repo.join("one.txt"), "1\n").expect("write");
        let first = core_status(&runner, &mut registry, &repo_id)
            .await
            .expect("first");
        let old_token = first.files[0].path_id.clone();

        // External change observed by the next read bumps the generation.
        std::fs::write(repo.join("two.txt"), "2\n").expect("write");
        let second = core_status(&runner, &mut registry, &repo_id)
            .await
            .expect("second");
        assert_eq!(second.files.len(), 2);
        let stale = registry.status_resolve(&repo_id, &old_token);
        assert!(matches!(
            stale,
            Err(ref e) if e.code == ErrorCode::STALE_STATE
        ));
        // Fresh tokens from the current listing still resolve.
        for file in &second.files {
            assert!(registry.status_resolve(&repo_id, &file.path_id).is_ok());
        }
    }

    #[tokio::test]
    async fn snapshot_counts_fill_only_when_trusted() {
        let (_root, repo) = temp_repo("counts");
        std::fs::write(repo.join("dirty.txt"), "x\n").expect("write");
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_session(&mut registry, &repo, TrustState::ReadOnly);

        let session = registry.get(&repo_id).expect("session").clone();
        let (staged, unstaged, conflicted) = snapshot_counts(
            &runner,
            &mut registry,
            &repo_id,
            &session.worktree_root,
            false,
        )
        .await;
        assert_eq!((staged, unstaged, conflicted), (None, None, None));

        let (staged, unstaged, conflicted) = snapshot_counts(
            &runner,
            &mut registry,
            &repo_id,
            &session.worktree_root,
            true,
        )
        .await;
        assert_eq!(staged, Some(0));
        assert_eq!(unstaged, Some(1));
        assert_eq!(conflicted, Some(0));
    }

    #[test]
    fn double_submit_dedups_same_payload_and_rejects_drift() {
        let mut registry = RepoRegistry::default();
        let first = registry
            .job_register(test_record("req-1", None), "payload-a")
            .expect("register");
        // Identical retry returns the same operation id.
        let retry = registry
            .job_register(test_record("req-1", None), "payload-a")
            .expect("dedup");
        assert_eq!(first.operation_id, retry.operation_id);
        // Same request id, different payload is a caller bug: reject.
        let drift = registry.job_register(test_record("req-1", None), "payload-b");
        assert!(matches!(
            drift,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));
    }

    #[test]
    fn close_is_blocked_while_a_job_runs_and_frees_after() {
        let mut registry = RepoRegistry::default();
        let repo_id = "repo-busy".to_string();
        registry
            .job_register(test_record("req-busy", Some(repo_id.clone())), "p")
            .expect("register");
        assert!(registry.has_active_operation(&repo_id));
        registry.job_set_state("op-req-busy", OperationState::Succeeded);
        assert!(!registry.has_active_operation(&repo_id));
    }

    #[test]
    fn operation_get_falls_back_for_missed_events_and_rejects_unknown() {
        let mut registry = RepoRegistry::default();
        registry
            .job_register(test_record("req-lookup", None), "p")
            .expect("register");
        // Listener-race fallback: query by operation id returns the record.
        let found = core_operation_get(&registry, "op-req-lookup").expect("found");
        assert_eq!(found.request_id, "req-lookup");
        assert_eq!(found.state, OperationState::Running);
        let missing = core_operation_get(&registry, "op-nope");
        assert!(matches!(
            missing,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));
        // Terminal records cannot be cancelled; running ones request it.
        // Unknown ids report Ok(false): nothing to kill, still not an error.
        assert!(core_operation_cancel(&mut registry, "op-req-lookup").expect("cancel"));
        assert!(!core_operation_cancel(&mut registry, "op-nope-2").expect("unknown"));
        registry.job_set_state("op-req-lookup", OperationState::Succeeded);
        assert!(!core_operation_cancel(&mut registry, "op-req-lookup").expect("no-op"));
    }

    #[tokio::test]
    async fn write_queues_serialize_per_common_dir() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let mut registry = RepoRegistry::default();
        let shared = registry.queue_for("/tmp/common-a");
        let second = registry.queue_for("/tmp/common-a");
        // Same common dir (linked worktrees) shares one queue ...
        assert!(Arc::ptr_eq(&shared, &second));
        // ... while another repo gets its own.
        let other = registry.queue_for("/tmp/common-b");
        assert!(!Arc::ptr_eq(&shared, &other));

        let counter = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let queue = registry.queue_for("/tmp/common-a");
            let counter = counter.clone();
            handles.push(tokio::spawn(async move {
                let _guard = queue.lock().await;
                let seen = counter.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                counter.fetch_sub(1, Ordering::SeqCst);
                seen
            }));
        }
        for handle in handles {
            handle.await.expect("join");
        }
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dto_shapes_match_the_typescript_contract() {
        // Contract §6: camelCase keys, optional fields skipped when absent.
        let file = ChangedFile {
            path_id: "repo:3:0".to_string(),
            display_path: "a.txt".to_string(),
            original_display_path: Some("old.txt".to_string()),
            index_status: "R".to_string(),
            worktree_status: " ".to_string(),
            kind: "unknown".to_string(),
            conflicted: false,
        };
        let value = serde_json::to_value(&file).expect("serialize");
        assert_eq!(
            value,
            serde_json::json!({
                "pathId": "repo:3:0",
                "displayPath": "a.txt",
                "originalDisplayPath": "old.txt",
                "indexStatus": "R",
                "worktreeStatus": " ",
                "kind": "unknown",
                "conflicted": false
            })
        );
        let record = test_record("req-shape", Some("repo-x".to_string()));
        let value = serde_json::to_value(&record).expect("serialize");
        assert_eq!(value["operationId"], serde_json::json!("op-req-shape"));
        assert_eq!(value["repoId"], serde_json::json!("repo-x"));
        assert_eq!(value["errorCode"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn rapid_refreshes_stay_consistent_under_storm() {
        let (_root, repo) = temp_repo("storm");
        for name in ["s1.txt", "s2.txt", "s3.txt"] {
            std::fs::write(repo.join(name), "x\n").expect("write");
        }
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_session(&mut registry, &repo, TrustState::Trusted);
        // Ten back-to-back reads: every one succeeds and the last listing's
        // tokens resolve (earlier generations are stale by design).
        let mut last_count = 0;
        for _ in 0..10 {
            let data = core_status(&runner, &mut registry, &repo_id)
                .await
                .expect("storm read");
            last_count = data.files.len();
        }
        assert_eq!(last_count, 3);
        let listing = registry.status_listing(&repo_id).expect("listing").clone();
        assert_eq!(listing.paths.len(), 3);
        for index in 0..3 {
            let token = registry.status_path_id(&repo_id, index).expect("token");
            assert!(registry.status_resolve(&repo_id, &token).is_ok());
        }
    }
}
