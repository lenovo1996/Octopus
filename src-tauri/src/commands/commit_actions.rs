//! Commit-row history actions behind the context menu (T18).
//!
//! The 16 commit-menu actions that used to render `Planned` now have typed
//! IPC commands. Rules mirror docs/05-git-engine.md:
//!
//! - Every mutation takes a `WriteContext` (`repoId` + `expectedVersion`),
//!   requires a trusted repo, serializes on the per-common-dir queue and
//!   ends with a version bump plus history invalidation.
//! - OIDs are validated against the session object format and must exist
//!   (`cat-file -e`). Refs/branches/tags go through `git check-ref-format`.
//! - Clean-worktree preconditions apply to checkout, cherry-pick, revert,
//!   merge, rebase and resets that touch the worktree. Nothing force-pushes,
//!   nothing runs `branch -D`, and reset-hard additionally consumes a
//!   single-use confirmation token.
//! - `merge_commit` reuses the app-origin merge flow so `merge_complete` and
//!   `merge_abort` keep working. Cherry-pick and revert land with
//!   `--no-commit` so the user reviews the staged result before committing.
//! - Amend-family actions (`reword`, `modify`, `edit-author`, `split`) are
//!   HEAD-scoped in v1: older commits go through `rebase_interactive_from`,
//!   which replays an explicit pick/squash/fixup/drop/reword plan through a
//!   scripted `GIT_SEQUENCE_EDITOR` (never an interactive editor).

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, ErrorCode, RecoveryAction, RepoSnapshot, RepoState, RequestId, TrustState,
};
use crate::git::{
    list_refs, read_status, validate_branch_name, GitRunner, NETWORK_TIMEOUT, READ_TIMEOUT,
    WRITE_TIMEOUT,
};
use crate::services::{MergeRecord, RepoRegistry, RepoSession};

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

fn check_oid(object_format: &str, oid: &str) -> Result<(), AppError> {
    let expected = if object_format == "sha256" { 64 } else { 40 };
    if oid.len() == expected && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(bad_request("Malformed object id"))
    }
}

fn git_failed(action: &str) -> AppError {
    AppError::new(
        ErrorCode::GIT_ERROR,
        format!("Git could not {action}"),
        RecoveryAction::Refresh,
        true,
    )
}

fn unsupported(message: &str) -> AppError {
    AppError::new(
        ErrorCode::UNSUPPORTED,
        message.to_string(),
        RecoveryAction::InspectState,
        false,
    )
}

/// Shared write-context gate: session exists, trusted, version matches.
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
    Ok(session)
}

async fn object_exists(runner: &GitRunner, cwd: &std::path::Path, oid: &str) -> bool {
    runner
        .run(cwd, &["cat-file", "-e", oid], READ_TIMEOUT)
        .await
        .map(|out| out.success)
        .unwrap_or(false)
}

async fn check_commit_oid(
    runner: &GitRunner,
    session: &RepoSession,
    oid: &str,
) -> Result<(), AppError> {
    check_oid(&session.object_format, oid)?;
    if !object_exists(runner, &session.worktree_root, oid).await {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            "This commit is no longer in the repository; refresh and retry",
            RecoveryAction::Refresh,
            true,
        ));
    }
    // The OID must be a commit, not a blob/tree/tag object.
    let kind = runner
        .run(
            &session.worktree_root,
            &["cat-file", "-t", oid],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("verify the commit"))?;
    if String::from_utf8_lossy(&kind.stdout).trim() != "commit" {
        return Err(bad_request("The selected object is not a commit"));
    }
    Ok(())
}

async fn head_oid(runner: &GitRunner, session: &RepoSession) -> Result<String, AppError> {
    let out = runner
        .run(&session.worktree_root, &["rev-parse", "HEAD"], READ_TIMEOUT)
        .await
        .map_err(|_| git_failed("read HEAD"))?;
    if !out.success {
        return Err(bad_request("This repository has no commits yet"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn head_branch(runner: &GitRunner, session: &RepoSession) -> Result<String, AppError> {
    let out = runner
        .run(
            &session.worktree_root,
            &["symbolic-ref", "--short", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the current branch"))?;
    if !out.success {
        return Err(bad_request(
            "Start from a branch HEAD; detached and unborn states cannot start this operation",
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn merge_head_live(runner: &GitRunner, session: &RepoSession) -> bool {
    runner
        .run(
            &session.worktree_root,
            &["rev-parse", "--verify", "MERGE_HEAD"],
            READ_TIMEOUT,
        )
        .await
        .map(|out| out.success)
        .unwrap_or(false)
}

async fn rebase_in_progress(runner: &GitRunner, session: &RepoSession) -> bool {
    for marker in ["rebase-merge", "rebase-apply"] {
        let out = runner
            .run(
                &session.worktree_root,
                &["rev-parse", "--git-path", marker],
                READ_TIMEOUT,
            )
            .await;
        if let Ok(out) = out {
            if out.success {
                let rel = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !rel.is_empty() {
                    // Resolve relative to the git dir, not the worktree.
                    let git_dir = runner
                        .run(
                            &session.worktree_root,
                            &["rev-parse", "--absolute-git-dir"],
                            READ_TIMEOUT,
                        )
                        .await;
                    if let Ok(git_dir) = git_dir {
                        if git_dir.success {
                            let base = std::path::PathBuf::from(
                                String::from_utf8_lossy(&git_dir.stdout).trim(),
                            );
                            if base.join(&rel).exists() || std::path::PathBuf::from(&rel).exists() {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

async fn require_no_merge_or_rebase(
    runner: &GitRunner,
    session: &RepoSession,
) -> Result<(), AppError> {
    if merge_head_live(runner, session).await {
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "A merge is already in progress; complete or abort it first",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    if rebase_in_progress(runner, session).await {
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "A rebase is already in progress; finish it in a terminal (git rebase --continue/--abort) first",
            RecoveryAction::InspectState,
            false,
        ));
    }
    Ok(())
}

async fn require_clean(
    runner: &GitRunner,
    session: &RepoSession,
    what: &str,
) -> Result<(), AppError> {
    let parsed = read_status(runner, &session.worktree_root)
        .await
        .map_err(|_| git_failed("read the worktree"))?;
    let counts = parsed.counts();
    if counts.staged != 0 || counts.unstaged != 0 || counts.conflicted != 0 {
        return Err(AppError::new(
            ErrorCode::DIRTY_WORKTREE,
            format!("{what} needs a clean worktree; stash or commit first"),
            RecoveryAction::InspectState,
            false,
        ));
    }
    Ok(())
}

async fn conflicted_count(runner: &GitRunner, session: &RepoSession) -> usize {
    read_status(runner, &session.worktree_root)
        .await
        .map(|parsed| parsed.counts().conflicted)
        .unwrap_or(0)
}

async fn fresh_snapshot(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
) -> Result<RepoSnapshot, AppError> {
    registry.bump(repo_id).ok_or_else(session_missing)?;
    registry.invalidate_history(repo_id);
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    super::repos::build_snapshot(runner, registry, &session).await
}

fn check_ref_format(runner_out_success: bool, what: &str) -> Result<(), AppError> {
    if runner_out_success {
        Ok(())
    } else {
        Err(bad_request(what))
    }
}

async fn ensure_ref_format(
    runner: &GitRunner,
    session: &RepoSession,
    full_ref: &str,
    what: &str,
) -> Result<(), AppError> {
    let out = runner
        .run(
            &session.worktree_root,
            &["check-ref-format", full_ref],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("validate the name"))?;
    check_ref_format(out.success, what)
}

// ---- checkout ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryCheckoutRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
}

async fn core_checkout(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "check out commits")?;
    check_commit_oid(runner, &session, oid).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_clean(runner, &session, "Checking out a commit").await?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["checkout", "--detach", oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("check out the commit"))?;
    if !out.success {
        return Err(git_failed("check out the commit"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- tag ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCreateAtRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub name: String,
}

async fn core_tag_create_at(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    name: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "create tags")?;
    check_commit_oid(runner, &session, oid).await?;
    let name = name.trim();
    if name.is_empty() || name.len() > 250 {
        return Err(bad_request("Tag name must be 1..=250 characters"));
    }
    if name.starts_with('-') || name.contains("..") || name.contains(' ') {
        return Err(bad_request("Tag name is not a valid ref name"));
    }
    ensure_ref_format(
        runner,
        &session,
        &format!("refs/tags/{name}"),
        "Tag name is not a valid ref name",
    )
    .await?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(&session.worktree_root, &["tag", name, oid], WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed("create the tag"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr);
        if detail.contains("already exists") {
            return Err(bad_request("A tag with this name already exists"));
        }
        return Err(git_failed("create the tag"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- push historical commit ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPushToRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub remote: String,
    pub dest_branch: String,
}

fn push_terminal_error(stderr: &str) -> AppError {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("authentication failed")
        || lower.contains("permission denied")
        || lower.contains("401")
        || lower.contains("403")
        || lower.contains("invalid username")
        || lower.contains("could not read username")
    {
        return AppError::new(
            ErrorCode::AUTH_REQUIRED,
            "The remote rejected the credentials; refresh the token or unlock the SSH agent, then retry",
            RecoveryAction::Authenticate,
            true,
        );
    }
    if lower.contains("could not resolve")
        || lower.contains("network")
        || lower.contains("timed out")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
    {
        return AppError::new(
            ErrorCode::NETWORK_ERROR,
            "The network request to the remote failed; check connectivity and retry",
            RecoveryAction::RetryRead,
            true,
        );
    }
    if lower.contains("non-fast-forward") || lower.contains("fetch first") {
        return AppError::new(
            ErrorCode::DIVERGED,
            "The remote branch moved ahead; fetch and merge before pushing",
            RecoveryAction::Refresh,
            true,
        );
    }
    git_failed("push the commit")
}

async fn core_history_push_to(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    remote: &str,
    dest_branch: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "push commits")?;
    check_commit_oid(runner, &session, oid).await?;
    let remote = remote.trim();
    if remote.is_empty() || remote.len() > 250 {
        return Err(bad_request("Choose an explicit remote to push to"));
    }
    let dest = dest_branch.trim();
    if dest.is_empty() || dest.len() > 250 {
        return Err(bad_request("Choose an explicit destination branch"));
    }
    ensure_ref_format(
        runner,
        &session,
        &format!("refs/heads/{dest}"),
        "Destination branch is not a valid ref name",
    )
    .await?;
    let remotes = runner
        .run(&session.worktree_root, &["remote"], READ_TIMEOUT)
        .await
        .map_err(|_| git_failed("list remotes"))?;
    let known = String::from_utf8_lossy(&remotes.stdout);
    if !known.lines().any(|line| line.trim() == remote) {
        return Err(bad_request(
            "Unknown remote; fetch the remote list and retry",
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let spec = format!("{oid}:refs/heads/{dest}");
    let out = runner
        .run(
            &session.worktree_root,
            &["push", remote, &spec],
            NETWORK_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("push the commit"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(push_terminal_error(&detail));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- cherry-pick (staged, --no-commit) ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOidRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
}

async fn core_cherry_pick_onto(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "cherry-pick")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_clean(runner, &session, "Cherry-picking").await?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["cherry-pick", "--no-commit", oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("cherry-pick the commit"))?;
    if out.success {
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    if conflicted_count(runner, &session).await > 0 {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "The cherry-pick stopped on conflicts; resolve them, then commit the staged result",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    let detail = String::from_utf8_lossy(&out.stderr);
    if detail.contains("is empty") || detail.contains("nothing to commit") {
        let _ = runner
            .run(
                &session.worktree_root,
                &["cherry-pick", "--quit"],
                WRITE_TIMEOUT,
            )
            .await;
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(bad_request(
            "This commit introduces no changes on top of HEAD",
        ));
    }
    let _ = runner
        .run(
            &session.worktree_root,
            &["cherry-pick", "--quit"],
            WRITE_TIMEOUT,
        )
        .await;
    let _ = fresh_snapshot(runner, registry, repo_id).await;
    Err(git_failed("cherry-pick the commit"))
}

// ---- revert (staged, --no-commit) ----

async fn core_revert_commit(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "revert commits")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_clean(runner, &session, "Reverting").await?;
    let head = head_oid(runner, &session).await?;
    if head == oid {
        return Err(bad_request(
            "Reverting HEAD would empty itself; reset or amend instead",
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["revert", "--no-commit", "--no-edit", oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("revert the commit"))?;
    if out.success {
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    if conflicted_count(runner, &session).await > 0 {
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "The revert stopped on conflicts; resolve them, then commit the staged result",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    let _ = runner
        .run(&session.worktree_root, &["revert", "--quit"], WRITE_TIMEOUT)
        .await;
    let _ = fresh_snapshot(runner, registry, repo_id).await;
    Err(git_failed("revert the commit"))
}

// ---- merge commit OID into current branch ----

async fn core_merge_commit(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "start merges")?;
    check_commit_oid(runner, &session, oid).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    let current = head_branch(runner, &session).await?;
    let head = head_oid(runner, &session).await?;
    require_clean(
        runner,
        &session,
        "Merges start from a clean worktree; stash or commit first",
    )
    .await?;

    if oid == head {
        registry.bump(repo_id).ok_or_else(session_missing)?;
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    // Never merge unrelated histories automatically.
    let base = runner
        .run(
            &session.worktree_root,
            &["merge-base", "HEAD", oid],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("compare the commits"))?;
    if !base.success {
        return Err(bad_request(
            "Unrelated histories are never merged automatically",
        ));
    }
    let ancestor = runner
        .run(
            &session.worktree_root,
            &["merge-base", "--is-ancestor", oid, "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("compare the commits"))?;
    if ancestor.success {
        registry.bump(repo_id).ok_or_else(session_missing)?;
        return fresh_snapshot(runner, registry, repo_id).await;
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    registry.merge_begin(
        repo_id,
        MergeRecord {
            pre_head: head,
            source_oid: oid.to_string(),
            source_label: format!("commit {}", &oid[..oid.len().min(7)]),
            current_label: current,
        },
    );
    registry.set_repo_state(repo_id, RepoState::Merging);
    let merged = runner
        .run(
            &session.worktree_root,
            &["merge", "--no-ff", "--no-commit", "--no-edit", oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("merge the commit"))?;
    if merged.success {
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    if conflicted_count(runner, &session).await > 0 {
        let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
        void_snapshot(&snapshot);
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "The merge stopped on conflicts; resolve them, then complete the merge",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    let aborted = runner
        .run(&session.worktree_root, &["merge", "--abort"], WRITE_TIMEOUT)
        .await;
    if aborted.map(|out| out.success).unwrap_or(false) && !merge_head_live(runner, &session).await {
        registry.merge_clear(repo_id);
        registry.set_repo_state(repo_id, RepoState::Normal);
        let _ = fresh_snapshot(runner, registry, repo_id).await;
        return Err(git_failed("merge the commit"));
    }
    let _ = fresh_snapshot(runner, registry, repo_id).await;
    Err(AppError::new(
        ErrorCode::GIT_ERROR,
        "The merge failed and automatic cleanup did not finish; inspect the repository before retrying",
        RecoveryAction::InspectState,
        true,
    ))
}

fn void_snapshot(_snapshot: &RepoSnapshot) {}

// ---- rebase current branch onto a commit ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseOntoRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub confirmation_token: String,
}

async fn core_rebase_onto(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "rebase")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_clean(runner, &session, "Rebasing").await?;
    registry.confirmation_consume(
        repo_id,
        session.version,
        "history_rebase",
        &[oid.to_string()],
        confirmation_token,
    )?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(&session.worktree_root, &["rebase", oid], WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed("rebase the branch"))?;
    if out.success {
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    let _ = fresh_snapshot(runner, registry, repo_id).await;
    Err(AppError::new(
        ErrorCode::CONFLICTS_PRESENT,
        "The rebase stopped on conflicts; finish it in a terminal (git status, resolve, git rebase --continue/--abort)",
        RecoveryAction::InspectState,
        false,
    ))
}

// ---- HEAD-scoped amend family ----

async fn require_head_oid(
    runner: &GitRunner,
    session: &RepoSession,
    oid: &str,
    action: &str,
) -> Result<(), AppError> {
    let head = head_oid(runner, session).await?;
    if head != oid {
        return Err(unsupported(&format!(
            "{action} supports only HEAD in v1; older commits go through Interactive rebase"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewordRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub subject: String,
    pub body: String,
}

async fn core_reword(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    subject: &str,
    body: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "reword commits")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_head_oid(runner, &session, oid, "Reword").await?;
    let subject = subject.trim();
    if subject.is_empty() || subject.chars().count() > 500 {
        return Err(bad_request(
            "Subject is required and limited to 500 characters",
        ));
    }
    if body.len() > 64 * 1024 {
        return Err(bad_request("Body is limited to 64 KiB"));
    }
    let mut message = subject.to_string();
    let body = body.trim();
    if !body.is_empty() {
        message.push_str("\n\n");
        message.push_str(body);
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run_with_stdin(
            &session.worktree_root,
            &["commit", "--amend", "-F", "-"],
            message.as_bytes(),
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("reword the commit"))?;
    if !out.success {
        return Err(git_failed("reword the commit"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

async fn core_modify(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "modify commits")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_head_oid(runner, &session, oid, "Modify").await?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    // Amending needs staged changes; an empty index would silently no-op.
    let staged = runner
        .run(
            &session.worktree_root,
            &["diff", "--cached", "--quiet"],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the index"))?;
    if staged.success {
        return Err(AppError::new(
            ErrorCode::EMPTY_INDEX,
            "Stage the follow-up changes first, then modify HEAD to fold them in",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let out = runner
        .run(
            &session.worktree_root,
            &["commit", "--amend", "--no-edit"],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("modify the commit"))?;
    if !out.success {
        return Err(git_failed("modify the commit"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditAuthorRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub name: String,
    pub email: String,
}

async fn core_edit_author(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    name: &str,
    email: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "edit authors")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_head_oid(runner, &session, oid, "Edit author").await?;
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() || email.is_empty() || !email.contains('@') {
        return Err(bad_request("Author needs a name and a valid email"));
    }
    let author = format!("{name} <{email}>");

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &[
                "commit",
                "--amend",
                "--no-edit",
                &format!("--author={author}"),
            ],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("edit the author"))?;
    if !out.success {
        return Err(git_failed("edit the author"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- split HEAD: uncommit into the worktree for piecewise re-commit ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub confirmation_token: String,
}

async fn core_split(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "split commits")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_head_oid(runner, &session, oid, "Split").await?;
    registry.confirmation_consume(
        repo_id,
        session.version,
        "history_split",
        &[oid.to_string()],
        confirmation_token,
    )?;
    let parent = runner
        .run(
            &session.worktree_root,
            &["rev-parse", &format!("{oid}^")],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the parent commit"))?;
    if !parent.success {
        return Err(bad_request("The root commit cannot be split this way"));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["reset", "--mixed", &format!("{oid}^")],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("split the commit"))?;
    if !out.success {
        return Err(git_failed("split the commit"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- move to branch: branch at OID with optional switch ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToBranchRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub name: String,
    pub switch_after_create: bool,
}

async fn core_move_to_branch(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    name: &str,
    switch_after: bool,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "move commits")?;
    check_commit_oid(runner, &session, oid).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    validate_branch_name(runner, &session.worktree_root, name)
        .await
        .map_err(|_| bad_request("Branch name is not valid"))?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["branch", name, oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("create the branch"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr);
        if detail.contains("already exists") {
            return Err(bad_request("A branch with this name already exists"));
        }
        return Err(git_failed("create the branch"));
    }
    if switch_after {
        require_clean(runner, &session, "Switching branches").await?;
        let refs = list_refs(runner, &session)
            .await
            .map_err(|_| git_failed("read refs"))?;
        let full = format!("refs/heads/{name}");
        let target = refs.iter().find(|r| r.full_name == full).ok_or_else(|| {
            AppError::new(
                ErrorCode::REF_INVALID,
                "Branch no longer exists",
                RecoveryAction::Refresh,
                true,
            )
        })?;
        if target.checked_out_elsewhere {
            return Err(AppError::new(
                ErrorCode::REF_INVALID,
                "Branch is checked out in another worktree",
                RecoveryAction::InspectState,
                false,
            ));
        }
        let switched = runner
            .run(&session.worktree_root, &["switch", name], WRITE_TIMEOUT)
            .await
            .map_err(|_| git_failed("switch branches"))?;
        if !switched.success {
            return Err(git_failed("switch branches"));
        }
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- resets ----

#[allow(clippy::too_many_arguments)]
async fn core_reset(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    mode: &str,
    confirmation_token: Option<&str>,
    action_name: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "reset")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    if mode == "--hard" {
        let token =
            confirmation_token.ok_or_else(|| bad_request("Hard reset needs a confirmation"))?;
        registry.confirmation_consume(
            repo_id,
            session.version,
            action_name,
            &[oid.to_string()],
            token,
        )?;
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(&session.worktree_root, &["reset", mode, oid], WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed("reset HEAD"))?;
    if !out.success {
        return Err(git_failed("reset HEAD"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetHardRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub confirmation_token: String,
}

// ---- scripted interactive rebase from a commit ----

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanEntry {
    pub oid: String,
    pub action: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseInteractiveRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub oid: String,
    pub plan: Vec<RebasePlanEntry>,
    pub confirmation_token: String,
}

fn plan_action(entry: &RebasePlanEntry) -> Result<&str, AppError> {
    match entry.action.as_str() {
        "pick" | "reword" | "squash" | "fixup" | "drop" => Ok(entry.action.as_str()),
        _ => Err(bad_request("Unknown rebase plan action")),
    }
}

async fn core_rebase_interactive(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    oid: &str,
    plan: &[RebasePlanEntry],
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = check_write_context(registry, repo_id, expected_version, "rebase")?;
    check_commit_oid(runner, &session, oid).await?;
    head_branch(runner, &session).await?;
    require_no_merge_or_rebase(runner, &session).await?;
    require_clean(runner, &session, "Rebasing").await?;
    if plan.is_empty() || plan.len() > 200 {
        return Err(bad_request("The rebase plan must list 1..=200 commits"));
    }
    if plan[0].oid != oid {
        return Err(bad_request(
            "The rebase plan must start at the selected commit",
        ));
    }
    if cfg!(not(unix)) {
        return Err(unsupported("Interactive rebase needs a Unix shell in v1"));
    }
    registry.confirmation_consume(
        repo_id,
        session.version,
        "history_rebase_interactive",
        &[oid.to_string()],
        confirmation_token,
    )?;

    // Validate every entry before touching the repo.
    let mut keeping = 0;
    for entry in plan {
        let action = plan_action(entry)?;
        check_commit_oid(runner, &session, &entry.oid).await?;
        if action == "reword" {
            let message = entry.message.as_deref().unwrap_or("").trim();
            if message.is_empty() || message.chars().count() > 500 {
                return Err(bad_request("Reword needs a message of 1..=500 characters"));
            }
        }
        if action != "drop" {
            keeping += 1;
        }
    }
    if keeping == 0 {
        return Err(bad_request("Dropping every commit would delete the branch"));
    }
    // The scripted editor overwrites the whole todo: anything in the range
    // but missing from the plan would be silently dropped. Require the plan
    // to list exactly the commits from the selection up to HEAD.
    let parent = runner
        .run(
            &session.worktree_root,
            &["rev-parse", &format!("{oid}^")],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the parent commit"))?;
    if !parent.success {
        return Err(bad_request(
            "The root commit cannot start an interactive rebase",
        ));
    }
    let parent_oid = String::from_utf8_lossy(&parent.stdout).trim().to_string();
    let range = runner
        .run(
            &session.worktree_root,
            &[
                "rev-list",
                "--topo-order",
                "--reverse",
                &format!("{parent_oid}..HEAD"),
            ],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the rebase range"))?;
    if !range.success {
        return Err(git_failed("read the rebase range"));
    }
    let expected: Vec<String> = String::from_utf8_lossy(&range.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let planned: Vec<&str> = plan.iter().map(|entry| entry.oid.as_str()).collect();
    let expected_refs: Vec<&str> = expected.iter().map(String::as_str).collect();
    if expected_refs != planned {
        return Err(AppError::new(
            ErrorCode::STALE_STATE,
            "The history changed or is not fully loaded; refresh and retry",
            RecoveryAction::Refresh,
            true,
        ));
    }

    // Stage a private script dir: the sequence editor overwrites the todo
    // file with our validated plan, and `exec` lines carry reword messages
    // through files (never argv). Best-effort cleanup afterwards.
    let script_dir = std::env::temp_dir().join(format!(
        "gitdock-rebase-{}-{}",
        std::process::id(),
        registry.get(repo_id).map(|s| s.version).unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&script_dir);
    std::fs::create_dir_all(&script_dir).map_err(|_| git_failed("prepare the rebase"))?;
    let mut todo = String::new();
    let mut msg_index = 0;
    for entry in plan {
        let action = plan_action(entry)?;
        let subject = runner
            .run(
                &session.worktree_root,
                &["log", "-1", "--format=%s", &entry.oid],
                READ_TIMEOUT,
            )
            .await
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .unwrap_or_default();
        match action {
            "pick" => todo.push_str(&format!("pick {} {subject}\n", entry.oid)),
            "drop" => todo.push_str(&format!("drop {} {subject}\n", entry.oid)),
            "squash" => todo.push_str(&format!("squash {} {subject}\n", entry.oid)),
            "fixup" => todo.push_str(&format!("fixup {} {subject}\n", entry.oid)),
            _ => {
                // reword = pick plus an amend carrying the new message.
                let path = script_dir.join(format!("reword-{msg_index}.txt"));
                msg_index += 1;
                let message = entry.message.as_deref().unwrap_or("").trim().to_string() + "\n";
                std::fs::write(&path, message).map_err(|_| git_failed("prepare the rebase"))?;
                todo.push_str(&format!("pick {} {subject}\n", entry.oid));
                todo.push_str(&format!(
                    "exec git commit --amend -F '{}'\n",
                    path.to_string_lossy()
                ));
            }
        }
    }
    let todo_path = script_dir.join("todo.txt");
    std::fs::write(&todo_path, &todo).map_err(|_| git_failed("prepare the rebase"))?;
    #[cfg(unix)]
    {
        let editor = format!("cp '{}' \"$1\"\n", todo_path.to_string_lossy());
        let editor_path = script_dir.join("sequence-editor.sh");
        std::fs::write(&editor_path, format!("#!/bin/sh\n{editor}"))
            .map_err(|_| git_failed("prepare the rebase"))?;
        let mut perms = std::fs::metadata(&editor_path)
            .map_err(|_| git_failed("prepare the rebase"))?
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o700);
        std::fs::set_permissions(&editor_path, perms)
            .map_err(|_| git_failed("prepare the rebase"))?;
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let editor_value = script_dir.join("sequence-editor.sh");
    let out = runner
        .run_with_env(
            &session.worktree_root,
            &["rebase", "-i", &parent_oid],
            WRITE_TIMEOUT,
            &[
                (
                    "GIT_SEQUENCE_EDITOR",
                    editor_value.to_string_lossy().as_ref(),
                ),
                ("GIT_EDITOR", "true"),
                ("GIT_TERMINAL_PROMPT", "0"),
            ],
        )
        .await
        .map_err(|_| git_failed("rebase the branch"))?;
    let _ = std::fs::remove_dir_all(&script_dir);
    if out.success {
        registry.invalidate_history(repo_id);
        return fresh_snapshot(runner, registry, repo_id).await;
    }
    let _ = fresh_snapshot(runner, registry, repo_id).await;
    Err(AppError::new(
        ErrorCode::CONFLICTS_PRESENT,
        "The interactive rebase stopped; finish it in a terminal (git status, resolve, GIT_EDITOR=true git rebase --continue, or git rebase --abort)",
        RecoveryAction::InspectState,
        false,
    ))
}

// ---- Tauri handlers ----

type CoreOid = for<'a> fn(
    &'a GitRunner,
    &'a mut RepoRegistry,
    &'a str,
    u64,
    &'a str,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>,
>;

fn wrap_oid(core: CoreOid) -> CoreOid {
    core
}

async fn run_oid_command(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
    core: CoreOid,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

fn boxed_checkout<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_checkout(runner, registry, repo_id, version, oid))
}

fn boxed_cherry_pick<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_cherry_pick_onto(
        runner, registry, repo_id, version, oid,
    ))
}

fn boxed_revert<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_revert_commit(runner, registry, repo_id, version, oid))
}

fn boxed_merge<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_merge_commit(runner, registry, repo_id, version, oid))
}

fn boxed_modify<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_modify(runner, registry, repo_id, version, oid))
}

fn boxed_reset_soft<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_reset(
        runner, registry, repo_id, version, oid, "--soft", None, "",
    ))
}

fn boxed_reset_mixed<'a>(
    runner: &'a GitRunner,
    registry: &'a mut RepoRegistry,
    repo_id: &'a str,
    version: u64,
    oid: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RepoSnapshot, AppError>> + Send + 'a>>
{
    Box::pin(core_reset(
        runner, registry, repo_id, version, oid, "--mixed", None, "",
    ))
}

#[tauri::command]
pub async fn history_checkout(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_checkout)).await
}

#[tauri::command]
pub async fn cherry_pick_onto(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_cherry_pick)).await
}

#[tauri::command]
pub async fn revert_commit(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_revert)).await
}

#[tauri::command]
pub async fn merge_commit(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_merge)).await
}

#[tauri::command]
pub async fn modify_commit(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_modify)).await
}

#[tauri::command]
pub async fn reset_soft(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_reset_soft)).await
}

#[tauri::command]
pub async fn reset_mixed(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryOidRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    run_oid_command(registry, request, wrap_oid(boxed_reset_mixed)).await
}

#[tauri::command]
pub async fn tag_create_at(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: TagCreateAtRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_tag_create_at(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            &request.name,
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
pub async fn history_push_to(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryPushToRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_history_push_to(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            &request.remote,
            &request.dest_branch,
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
pub async fn rebase_onto(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RebaseOntoRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_rebase_onto(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
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

#[tauri::command]
pub async fn reword_message(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RewordRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_reword(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            &request.subject,
            &request.body,
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
pub async fn edit_author(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: EditAuthorRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_edit_author(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            &request.name,
            &request.email,
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
pub async fn split_commit(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: SplitRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_split(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
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

#[tauri::command]
pub async fn move_to_branch(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: MoveToBranchRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_move_to_branch(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            request.name.trim(),
            request.switch_after_create,
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
pub async fn reset_hard(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ResetHardRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_reset(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            "--hard",
            Some(&request.confirmation_token),
            "history_reset_hard",
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
pub async fn rebase_interactive_from(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RebaseInteractiveRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_rebase_interactive(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.oid,
            &request.plan,
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
        cherry_pick_onto, edit_author, history_checkout, history_push_to, merge_commit,
        modify_commit, move_to_branch, rebase_interactive_from, rebase_onto, reset_hard,
        reset_mixed, reset_soft, revert_commit, reword_message, split_commit, tag_create_at,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::commit::tests::{git, open_repo, temp_repo};
    use crate::domain::TrustState;
    use std::process::Command as StdCommand;

    fn oid_of(repo: &std::path::Path, rev: &str) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["rev-parse", rev])
            .output()
            .expect("rev-parse");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn version_of(registry: &RepoRegistry, repo_id: &str) -> u64 {
        registry.get(repo_id).expect("session").version
    }

    fn commit_file(repo: &std::path::Path, name: &str, content: &str, message: &str) {
        std::fs::write(repo.join(name), content).expect("write");
        git(repo, &["add", name]);
        git(repo, &["commit", "-m", message]);
    }

    fn issue_token(registry: &mut RepoRegistry, repo_id: &str, action: &str, oid: &str) -> String {
        let version = version_of(registry, repo_id);
        let (token, _) =
            registry.confirmation_issue(repo_id, version, action, vec![oid.to_string()]);
        token
    }

    #[tokio::test]
    async fn checkout_detaches_at_exact_oid() {
        let (_dir, repo) = temp_repo("actions-checkout");
        commit_file(&repo, "a.txt", "one\n", "first");
        commit_file(&repo, "a.txt", "two\n", "second");
        let first = oid_of(&repo, "HEAD~1");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        let snapshot = core_checkout(&runner, &mut registry, &repo_id, version, &first)
            .await
            .expect("checkout");
        assert!(matches!(
            snapshot.head,
            crate::domain::HeadState::Detached { .. }
        ));
        assert_eq!(oid_of(&repo, "HEAD"), first);
    }

    #[tokio::test]
    async fn tag_create_rejects_duplicates() {
        let (_dir, repo) = temp_repo("actions-tag");
        commit_file(&repo, "a.txt", "one\n", "first");
        let head = oid_of(&repo, "HEAD");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        core_tag_create_at(&runner, &mut registry, &repo_id, version, &head, "v1")
            .await
            .expect("tag");
        let version = version_of(&registry, &repo_id);
        let err = core_tag_create_at(&runner, &mut registry, &repo_id, version, &head, "v1")
            .await
            .expect_err("duplicate tag");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn cherry_pick_stages_without_committing() {
        let (_dir, repo) = temp_repo("actions-pick");
        commit_file(&repo, "base.txt", "base\n", "base");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "pick.txt", "picked\n", "picked change");
        let picked = oid_of(&repo, "HEAD");
        git(&repo, &["checkout", "main"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        core_cherry_pick_onto(&runner, &mut registry, &repo_id, version, &picked)
            .await
            .expect("cherry-pick");
        // Staged, not committed: HEAD unchanged, index holds the file.
        assert_eq!(oid_of(&repo, "HEAD"), oid_of(&repo, "main"));
        let staged = StdCommand::new("git")
            .current_dir(&repo)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .expect("diff");
        assert!(String::from_utf8_lossy(&staged.stdout).contains("pick.txt"));
    }

    #[tokio::test]
    async fn cherry_pick_conflict_reports_resolve_path() {
        let (_dir, repo) = temp_repo("actions-pick-conflict");
        commit_file(&repo, "a.txt", "base\n", "base");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "a.txt", "feature\n", "feature change");
        let picked = oid_of(&repo, "HEAD");
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "a.txt", "main\n", "main change");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        let err = core_cherry_pick_onto(&runner, &mut registry, &repo_id, version, &picked)
            .await
            .expect_err("conflict");
        assert_eq!(err.code, ErrorCode::CONFLICTS_PRESENT);
    }

    #[tokio::test]
    async fn resets_move_head_with_documented_side_effects() {
        let (_dir, repo) = temp_repo("actions-reset");
        commit_file(&repo, "a.txt", "one\n", "first");
        commit_file(&repo, "a.txt", "two\n", "second");
        let first = oid_of(&repo, "HEAD~1");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);

        // Soft reset keeps the second commit's content staged.
        let version = version_of(&registry, &repo_id);
        core_reset(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &first,
            "--soft",
            None,
            "",
        )
        .await
        .expect("soft");
        assert_eq!(oid_of(&repo, "HEAD"), first);

        // Mixed reset needs a second commit again; hard reset destroys work.
        commit_file(&repo, "b.txt", "b\n", "third");
        // Uncommitted tracked change: hard reset restores the target's bytes.
        std::fs::write(repo.join("a.txt"), "uncommitted\n").expect("write");
        let target = oid_of(&repo, "HEAD~1");
        let token = issue_token(&mut registry, &repo_id, "history_reset_hard", &target);
        let version = version_of(&registry, &repo_id);
        core_reset(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &target,
            "--hard",
            Some(&token),
            "history_reset_hard",
        )
        .await
        .expect("hard");
        assert_eq!(oid_of(&repo, "HEAD"), target);
        // b.txt did not exist at the target; the uncommitted a.txt edit is gone.
        assert!(!repo.join("b.txt").exists());
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).expect("read"),
            "one\n"
        );

        // Hard reset without a token fails closed.
        let version = version_of(&registry, &repo_id);
        let err = core_reset(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &target,
            "--hard",
            None,
            "history_reset_hard",
        )
        .await
        .expect_err("token required");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn reword_is_head_scoped() {
        let (_dir, repo) = temp_repo("actions-reword");
        commit_file(&repo, "a.txt", "one\n", "first");
        commit_file(&repo, "a.txt", "two\n", "second");
        let first = oid_of(&repo, "HEAD~1");
        let head = oid_of(&repo, "HEAD");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        core_reword(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &head,
            "second v2",
            "",
        )
        .await
        .expect("reword head");
        let log = StdCommand::new("git")
            .current_dir(&repo)
            .args(["log", "-1", "--format=%s"])
            .output()
            .expect("log");
        assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "second v2");

        let version = version_of(&registry, &repo_id);
        let err = core_reword(&runner, &mut registry, &repo_id, version, &first, "x", "")
            .await
            .expect_err("non-head");
        assert_eq!(err.code, ErrorCode::UNSUPPORTED);
    }

    #[tokio::test]
    async fn interactive_rebase_drops_and_rewords() {
        let (_dir, repo) = temp_repo("actions-rebase-i");
        commit_file(&repo, "a.txt", "one\n", "first");
        commit_file(&repo, "b.txt", "b\n", "drop me");
        commit_file(&repo, "c.txt", "c\n", "keep me");
        let drop_oid = oid_of(&repo, "HEAD~1");
        let keep_oid = oid_of(&repo, "HEAD");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let token = issue_token(
            &mut registry,
            &repo_id,
            "history_rebase_interactive",
            &drop_oid,
        );
        let version = version_of(&registry, &repo_id);
        core_rebase_interactive(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &drop_oid,
            &[
                RebasePlanEntry {
                    oid: drop_oid.clone(),
                    action: "drop".to_string(),
                    message: None,
                },
                RebasePlanEntry {
                    oid: keep_oid.clone(),
                    action: "reword".to_string(),
                    message: Some("kept and reworded".to_string()),
                },
            ],
            &token,
        )
        .await
        .expect("rebase");
        let log = StdCommand::new("git")
            .current_dir(&repo)
            .args(["log", "--format=%s"])
            .output()
            .expect("log");
        let subjects = String::from_utf8_lossy(&log.stdout).into_owned();
        assert!(subjects.contains("kept and reworded"));
        assert!(!subjects.contains("drop me"));
        assert!(!repo.join("b.txt").exists());
    }

    #[tokio::test]
    async fn merge_commit_conflict_keeps_app_flow() {
        let (_dir, repo) = temp_repo("actions-merge");
        commit_file(&repo, "a.txt", "base\n", "base");
        git(&repo, &["checkout", "-b", "side"]);
        commit_file(&repo, "a.txt", "side\n", "side change");
        let side = oid_of(&repo, "HEAD");
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "a.txt", "main\n", "main change");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = version_of(&registry, &repo_id);
        let err = core_merge_commit(&runner, &mut registry, &repo_id, version, &side)
            .await
            .expect_err("conflict");
        assert_eq!(err.code, ErrorCode::CONFLICTS_PRESENT);
        // The app-origin record survives so merge_abort stays available.
        assert!(registry.merge_get(&repo_id).is_some());
    }
}
