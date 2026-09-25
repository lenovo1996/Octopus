//! `stash_list` / `stash_save` / `stash_apply` (T12).
//!
//! Stash safety rules:
//!
//! - Tracked-only by default; untracked files enter the stash only with an
//!   explicit `includeUntracked` flag. Ignored files are never stashed.
//! - Stash items are identified by OID. `stash@{n}` indices are resolved to
//!   an OID immediately before acting and checked against the OID the UI
//!   saw; a reorder underneath fails with `STALE_STATE` instead of touching
//!   the wrong entry.
//! - Apply never passes `--index`: the pre-existing staged state is not
//!   promised back.
//! - Pop is apply-then-drop: the drop runs only after a successful apply
//!   and only when the entry still resolves to the applied OID. A conflict
//!   keeps the stash. A failed drop after a successful apply reports a
//!   partial success (`retained: true` plus `dropError`) and never re-applies.
//! - A conflicted apply marks the snapshot `stashConflict`. There is no
//!   `merge_complete`/`merge_abort` for stash conflicts: resolve and commit
//!   normally once the index is clean.

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, ErrorCode, RecoveryAction, RepoSnapshot, RepoState, RequestId, TrustState,
};
use crate::git::{runner::GitOutput, GitRunner, READ_TIMEOUT, WRITE_TIMEOUT};
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
        "Trust this repository to stash changes",
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

fn git_failed() -> AppError {
    AppError::new(
        ErrorCode::GIT_ERROR,
        "Git could not finish the stash operation",
        RecoveryAction::Refresh,
        true,
    )
}

/// One stash entry. `stashId` is the display index (`stash@{n}`); `oid` is
/// the identity the UI must hand back so reorder races fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub stash_id: String,
    pub oid: String,
    pub label: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSaveResult {
    pub oid: Option<String>,
    pub snapshot: RepoSnapshot,
    pub no_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashApplyResult {
    pub snapshot: RepoSnapshot,
    /// True when the entry is still in the stash list (conflict, apply mode,
    /// or a partial pop whose drop failed).
    pub retained: bool,
    /// True when the apply left unmerged entries behind.
    pub conflicted: bool,
    /// Set only for the partial pop: apply succeeded, drop did not.
    pub drop_error: Option<AppError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StashApplyMode {
    Apply,
    Pop,
}

/// `stash@{n}` shape check only; the OID check happens at action time.
fn parse_stash_index(stash_id: &str) -> Option<usize> {
    let inner = stash_id.strip_prefix("stash@{")?.strip_suffix('}')?;
    if inner.is_empty() || !inner.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    inner.parse().ok()
}

fn bad_stash_id() -> AppError {
    bad_request("Unknown stash entry; refresh the stash list and retry")
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

/// Resolve `stash@{n}` to its current OID. `None` when the entry is gone.
async fn resolve_oid(
    runner: &GitRunner,
    cwd: &std::path::Path,
    stash_id: &str,
) -> Result<Option<String>, AppError> {
    let out = run_git(
        runner,
        cwd,
        &[
            "rev-parse".to_string(),
            "--verify".to_string(),
            stash_id.to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    ))
}

async fn core_list(runner: &GitRunner, cwd: &std::path::Path) -> Result<Vec<StashEntry>, AppError> {
    let out = run_git(
        runner,
        cwd,
        &[
            "stash".to_string(),
            "list".to_string(),
            "--format=%gd%x00%H%x00%ct%x00%gs".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !out.success {
        return Err(git_failed());
    }
    let mut entries = Vec::new();
    for record in String::from_utf8_lossy(&out.stdout).split('\n') {
        if record.is_empty() {
            continue;
        }
        // Subject last: a multiline message only appends unparseable rows.
        let mut parts = record.splitn(4, '\0');
        let (Some(id), Some(oid), Some(created), Some(label)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if parse_stash_index(id).is_none() || oid.len() < 7 {
            continue;
        }
        entries.push(StashEntry {
            stash_id: id.to_string(),
            oid: oid.to_string(),
            label: label.to_string(),
            created_at: created.parse().unwrap_or(0),
        });
    }
    Ok(entries)
}

/// True when there is anything `stash push` would take.
async fn worktree_dirty(
    runner: &GitRunner,
    cwd: &std::path::Path,
    include_untracked: bool,
) -> Result<bool, AppError> {
    // `diff --quiet` exits 1 on differences: `success == false` is data.
    let tracked = run_git(
        runner,
        cwd,
        &["diff".to_string(), "--quiet".to_string()],
        READ_TIMEOUT,
    )
    .await?;
    if !tracked.success {
        return Ok(true);
    }
    let staged = run_git(
        runner,
        cwd,
        &[
            "diff".to_string(),
            "--cached".to_string(),
            "--quiet".to_string(),
        ],
        READ_TIMEOUT,
    )
    .await?;
    if !staged.success {
        return Ok(true);
    }
    if include_untracked {
        let others = run_git(
            runner,
            cwd,
            &[
                "ls-files".to_string(),
                "--others".to_string(),
                "--exclude-standard".to_string(),
            ],
            READ_TIMEOUT,
        )
        .await?;
        if !others.stdout.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn has_unmerged(runner: &GitRunner, cwd: &std::path::Path) -> Result<bool, AppError> {
    let out = run_git(
        runner,
        cwd,
        &["ls-files".to_string(), "-u".to_string()],
        READ_TIMEOUT,
    )
    .await?;
    Ok(!out.stdout.is_empty())
}

fn check_write_context(
    registry: &RepoRegistry,
    repo_id: &str,
    expected_version: u64,
) -> Result<crate::services::RepoSession, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
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

async fn core_save(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    message: &str,
    include_untracked: bool,
) -> Result<StashSaveResult, AppError> {
    if message.chars().count() > 500 {
        return Err(bad_request("Stash message must be 500 characters or fewer"));
    }
    let session = check_write_context(registry, repo_id, expected_version)?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    if !worktree_dirty(runner, &session.worktree_root, include_untracked).await? {
        registry.bump(repo_id).ok_or_else(session_missing)?;
        let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
        let snapshot = super::repos::build_snapshot(runner, registry, &session).await?;
        return Ok(StashSaveResult {
            oid: None,
            snapshot,
            no_change: true,
        });
    }

    let mut argv = vec!["stash".to_string(), "push".to_string()];
    if !message.is_empty() {
        argv.push("-m".to_string());
        argv.push(message.to_string());
    }
    if include_untracked {
        // Never `--all`: ignored files stay out of the stash in MVP.
        argv.push("--include-untracked".to_string());
    }
    let out = run_git(runner, &session.worktree_root, &argv, WRITE_TIMEOUT).await?;
    if !out.success {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed());
    }
    let oid = resolve_oid(runner, &session.worktree_root, "stash@{0}").await?;
    registry.set_repo_state(repo_id, RepoState::Normal);
    registry.invalidate_history(repo_id);
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(StashSaveResult {
        oid,
        snapshot,
        no_change: false,
    })
}

async fn core_apply(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    stash_id: &str,
    expected_oid: &str,
    mode: StashApplyMode,
) -> Result<StashApplyResult, AppError> {
    if parse_stash_index(stash_id).is_none() {
        return Err(bad_stash_id());
    }
    let session = check_write_context(registry, repo_id, expected_version)?;
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    // Identity check at action time: a reorder underneath fails closed.
    let actual = resolve_oid(runner, &session.worktree_root, stash_id).await?;
    if actual.as_deref() != Some(expected_oid) {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(stale_state());
    }

    // Never `--index`: the pre-existing staged state is not promised back.
    let applied = run_git(
        runner,
        &session.worktree_root,
        &[
            "stash".to_string(),
            "apply".to_string(),
            stash_id.to_string(),
        ],
        WRITE_TIMEOUT,
    )
    .await?;
    if !applied.success {
        if has_unmerged(runner, &session.worktree_root).await? {
            registry.set_repo_state(repo_id, RepoState::StashConflict);
            registry.invalidate_history(repo_id);
            let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
            return Ok(StashApplyResult {
                snapshot,
                retained: true,
                conflicted: true,
                drop_error: None,
            });
        }
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed());
    }

    let mut retained = mode == StashApplyMode::Apply;
    let mut drop_error = None;
    if mode == StashApplyMode::Pop {
        let dropped = run_git(
            runner,
            &session.worktree_root,
            &[
                "stash".to_string(),
                "drop".to_string(),
                stash_id.to_string(),
            ],
            WRITE_TIMEOUT,
        )
        .await?;
        // Verify the drop took the entry we applied, not a shifted neighbor.
        let remaining = core_list(runner, &session.worktree_root).await?;
        if !dropped.success || remaining.iter().any(|e| e.oid == expected_oid) {
            retained = true;
            drop_error = Some(AppError::new(
                ErrorCode::GIT_ERROR,
                "Stash applied, but dropping the entry failed; it was kept",
                RecoveryAction::Refresh,
                true,
            ));
        }
    }
    registry.set_repo_state(repo_id, RepoState::Normal);
    registry.invalidate_history(repo_id);
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(StashApplyResult {
        snapshot,
        retained,
        conflicted: false,
        drop_error,
    })
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashListRequest {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSaveRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub include_untracked: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashApplyRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub stash_id: String,
    pub expected_oid: String,
    pub mode: StashApplyMode,
}

// ---- Commands ----

#[tauri::command]
pub async fn stash_list(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: StashListRequest,
) -> Result<ApiResult<Vec<StashEntry>>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        let session = registry
            .get(&request.repo_id)
            .ok_or_else(session_missing)?
            .clone();
        core_list(&runner, &session.worktree_root).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn stash_save(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: StashSaveRequest,
) -> Result<ApiResult<StashSaveResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_save(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.message,
            request.include_untracked,
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
pub async fn stash_apply(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: StashApplyRequest,
) -> Result<ApiResult<StashApplyResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_apply(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.stash_id,
            &request.expected_oid,
            request.mode,
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
        stash_apply, stash_list, stash_save, StashApplyMode, StashApplyRequest, StashApplyResult,
        StashEntry, StashListRequest, StashSaveRequest, StashSaveResult,
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

    fn temp_repo(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("gitdock-t12-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
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

    async fn list(runner: &GitRunner, registry: &RepoRegistry, repo_id: &str) -> Vec<StashEntry> {
        let session = registry.get(repo_id).expect("session").clone();
        core_list(runner, &session.worktree_root)
            .await
            .expect("list")
    }

    #[tokio::test]
    async fn save_defaults_to_tracked_only_and_reports_no_change() {
        let (_dir, repo) = temp_repo("tracked-only");
        std::fs::write(repo.join("tracked.txt"), "v1\n").expect("write");
        git(&repo, &["add", "tracked.txt"]);
        git(&repo, &["commit", "-m", "add tracked"]);
        std::fs::write(repo.join("tracked.txt"), "v2\n").expect("write");
        std::fs::write(repo.join("new.txt"), "untracked\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);

        let v = version_of(&registry, &repo_id);
        let saved = core_save(&runner, &mut registry, &repo_id, v, "wip", false)
            .await
            .expect("save");
        assert!(!saved.no_change);
        assert!(saved.oid.map(|o| o.len() == 40).unwrap_or(false));
        // Tracked change stashed away; the untracked file stays put.
        assert_eq!(
            std::fs::read(repo.join("tracked.txt")).expect("read"),
            b"v1\n"
        );
        assert!(repo.join("new.txt").exists());

        // Only the untracked file left: a tracked-only save is a no-change.
        let v = version_of(&registry, &repo_id);
        let again = core_save(&runner, &mut registry, &repo_id, v, "wip", false)
            .await
            .expect("save");
        assert!(again.no_change);
        assert_eq!(again.oid, None);
    }

    #[tokio::test]
    async fn save_with_include_untracked_takes_new_files() {
        let (_dir, repo) = temp_repo("untracked");
        std::fs::write(repo.join("tracked.txt"), "v1\n").expect("write");
        git(&repo, &["add", "tracked.txt"]);
        git(&repo, &["commit", "-m", "add tracked"]);
        std::fs::write(repo.join("new.txt"), "untracked\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);

        let v = version_of(&registry, &repo_id);
        let saved = core_save(&runner, &mut registry, &repo_id, v, "with untracked", true)
            .await
            .expect("save");
        assert!(!saved.no_change);
        assert!(!repo.join("new.txt").exists());
        assert_eq!(list(&runner, &registry, &repo_id).await.len(), 1);
    }

    #[tokio::test]
    async fn apply_retains_and_pop_drops_the_entry() {
        let (_dir, repo) = temp_repo("apply-pop");
        std::fs::write(repo.join("f.txt"), "v1\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);
        std::fs::write(repo.join("f.txt"), "v2\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let v = version_of(&registry, &repo_id);
        let saved = core_save(&runner, &mut registry, &repo_id, v, "wip", false)
            .await
            .expect("save");
        let oid = saved.oid.expect("oid");

        // Apply keeps the entry and restores the bytes.
        let v = version_of(&registry, &repo_id);
        let applied = core_apply(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "stash@{0}",
            &oid,
            StashApplyMode::Apply,
        )
        .await
        .expect("apply");
        assert!(applied.retained && !applied.conflicted && applied.drop_error.is_none());
        assert_eq!(std::fs::read(repo.join("f.txt")).expect("read"), b"v2\n");
        assert_eq!(list(&runner, &registry, &repo_id).await.len(), 1);

        // Pop drops exactly the applied entry.
        git(&repo, &["checkout", "--", "f.txt"]);
        let v = version_of(&registry, &repo_id);
        let popped = core_apply(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "stash@{0}",
            &oid,
            StashApplyMode::Pop,
        )
        .await
        .expect("pop");
        assert!(!popped.retained && !popped.conflicted && popped.drop_error.is_none());
        assert_eq!(std::fs::read(repo.join("f.txt")).expect("read"), b"v2\n");
        assert!(list(&runner, &registry, &repo_id).await.is_empty());
    }

    #[tokio::test]
    async fn pop_conflict_keeps_the_stash_and_marks_conflict_state() {
        let (_dir, repo) = temp_repo("conflict");
        std::fs::write(repo.join("f.txt"), "base\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        std::fs::write(repo.join("f.txt"), "stashed\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        let v = version_of(&registry, &repo_id);
        let saved = core_save(&runner, &mut registry, &repo_id, v, "wip", false)
            .await
            .expect("save");
        let oid = saved.oid.expect("oid");

        // Diverge the same line in a new commit so the apply merges into a
        // clean worktree and leaves unmerged entries behind.
        std::fs::write(repo.join("f.txt"), "diverged\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "diverge"]);
        let v = version_of(&registry, &repo_id);
        let result = core_apply(
            &runner,
            &mut registry,
            &repo_id,
            v,
            "stash@{0}",
            &oid,
            StashApplyMode::Pop,
        )
        .await
        .expect("conflict is a result, not an error");
        assert!(result.retained && result.conflicted);
        assert_eq!(registry.state_of(&repo_id), RepoState::StashConflict);
        assert_eq!(result.snapshot.state, RepoState::StashConflict);
        // The stash survived the failed pop.
        let entries = list(&runner, &registry, &repo_id).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].oid, oid);
    }

    #[tokio::test]
    async fn reordered_stash_fails_closed_on_oid_mismatch() {
        let (_dir, repo) = temp_repo("reorder");
        std::fs::write(repo.join("f.txt"), "v1\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "add f"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo);
        for (content, msg) in [("v2\n", "first"), ("v3\n", "second")] {
            std::fs::write(repo.join("f.txt"), content).expect("write");
            let v = version_of(&registry, &repo_id);
            core_save(&runner, &mut registry, &repo_id, v, msg, false)
                .await
                .expect("save");
        }
        let entries = list(&runner, &registry, &repo_id).await;
        assert_eq!(entries.len(), 2);
        // stash@{0} is "second"; handing it the "first" OID must refuse.
        let v = version_of(&registry, &repo_id);
        let err = core_apply(
            &runner,
            &mut registry,
            &repo_id,
            v,
            &entries[0].stash_id,
            &entries[1].oid,
            StashApplyMode::Pop,
        )
        .await
        .expect_err("oid mismatch must fail");
        assert_eq!(err.code, ErrorCode::STALE_STATE);
        // Nothing applied, nothing dropped.
        assert_eq!(list(&runner, &registry, &repo_id).await.len(), 2);
        assert_eq!(std::fs::read(repo.join("f.txt")).expect("read"), b"v1\n");
    }
}
