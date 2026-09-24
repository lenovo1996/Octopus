//! Branch workflows: create / switch / safe delete + confirmations (T10).
//!
//! Every mutation takes a `WriteContext`, serializes on the per-common-dir
//! queue, and ends with a version bump plus history invalidation. Delete is
//! safe-only (`-d`, never `-D`) behind a single-use confirmation token bound
//! to repo/version/action/target. Switch demands a clean worktree and never
//! force-moves refs.

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, BranchCreateResult, ConfirmationDetails, ErrorCode, RecoveryAction,
    RefItem, RepoSnapshot, RequestId, TrustState,
};
use crate::git::{
    list_refs, read_status, validate_branch_name, GitRunner, NETWORK_TIMEOUT, READ_TIMEOUT,
    WRITE_TIMEOUT,
};
use crate::services::{RepoRegistry, RepoSession};

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

fn short_name(full: &str) -> Option<&str> {
    full.strip_prefix("refs/heads/")
}

async fn object_exists(runner: &GitRunner, cwd: &std::path::Path, oid: &str) -> bool {
    runner
        .run(cwd, &["cat-file", "-e", oid], READ_TIMEOUT)
        .await
        .map(|out| out.success)
        .unwrap_or(false)
}

async fn is_merged(runner: &GitRunner, cwd: &std::path::Path, oid: &str) -> bool {
    runner
        .run(
            cwd,
            &["merge-base", "--is-ancestor", oid, "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .map(|out| out.success)
        .unwrap_or(false)
}

async fn is_clean(runner: &GitRunner, cwd: &std::path::Path) -> Result<bool, AppError> {
    let parsed = read_status(runner, cwd)
        .await
        .map_err(|_| git_failed("read the worktree"))?;
    let counts = parsed.counts();
    Ok(counts.staged == 0 && counts.unstaged == 0 && counts.conflicted == 0)
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

fn find_ref(refs: &[RefItem], ref_id: &str) -> Result<RefItem, AppError> {
    refs.iter()
        .find(|r| r.ref_id == ref_id)
        .cloned()
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::REF_INVALID,
                "Unknown branch reference",
                RecoveryAction::Refresh,
                true,
            )
        })
}

// ---- create ----

async fn core_branch_create(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    name: &str,
    start_oid: &str,
    switch_after: bool,
) -> Result<BranchCreateResult, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("create branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    validate_branch_name(runner, &session.worktree_root, name).await?;
    check_oid(&session.object_format, start_oid)?;
    if !object_exists(runner, &session.worktree_root, start_oid).await {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            "Start commit does not exist",
            RecoveryAction::Refresh,
            true,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["branch", name, start_oid],
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

    let mut switched = false;
    let mut switch_error = None;
    if switch_after {
        match switch_to(runner, registry, repo_id, name).await {
            Ok(()) => switched = true,
            Err(error) => switch_error = Some(error),
        }
    }
    let snapshot = fresh_snapshot(runner, registry, repo_id).await?;
    Ok(BranchCreateResult {
        snapshot: Box::new(snapshot),
        switched,
        switch_error,
    })
}

/// Checkout preconditions shared by create+switch and switch: clean
/// worktree, target not checked out elsewhere. Runs under the caller's
/// queue lock.
async fn switch_to(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    name: &str,
) -> Result<(), AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let refs = list_refs(runner, &session).await?;
    let full = format!("refs/heads/{name}");
    let target = refs.iter().find(|r| r.full_name == full).ok_or_else(|| {
        AppError::new(
            ErrorCode::REF_INVALID,
            "Branch no longer exists",
            RecoveryAction::Refresh,
            true,
        )
    })?;
    if target.current {
        return Ok(());
    }
    if target.checked_out_elsewhere {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            "Branch is checked out in another worktree",
            RecoveryAction::InspectState,
            false,
        ));
    }
    if !is_clean(runner, &session.worktree_root).await? {
        return Err(AppError::new(
            ErrorCode::DIRTY_WORKTREE,
            "Stash or commit your changes before switching branches",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let out = runner
        .run(&session.worktree_root, &["switch", name], WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed("switch branches"))?;
    if !out.success {
        return Err(git_failed("switch branches"));
    }
    Ok(())
}

// ---- switch ----

async fn core_branch_switch(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
    new_local_name: Option<&str>,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("switch branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let refs = list_refs(runner, &session).await?;
    let target = find_ref(&refs, ref_id)?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    if target.kind == "local" {
        let name = short_name(&target.full_name)
            .ok_or_else(|| bad_request("Only local branches can be switched directly"))?;
        switch_to(runner, registry, repo_id, name).await?;
    } else {
        // Remote ref: explicit local tracking branch, never an implicit name.
        let new_name = new_local_name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| bad_request("Choose a local name to track this remote branch"))?;
        validate_branch_name(runner, &session.worktree_root, new_name).await?;
        if !is_clean(runner, &session.worktree_root).await? {
            return Err(AppError::new(
                ErrorCode::DIRTY_WORKTREE,
                "Stash or commit your changes before switching branches",
                RecoveryAction::InspectState,
                false,
            ));
        }
        let out = runner
            .run(
                &session.worktree_root,
                &["switch", "-c", new_name, &target.full_name],
                WRITE_TIMEOUT,
            )
            .await
            .map_err(|_| git_failed("switch branches"))?;
        if !out.success {
            return Err(git_failed("switch branches"));
        }
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- delete + confirmations ----

async fn core_confirmation_prepare(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    action: &str,
    targets: &[String],
) -> Result<ConfirmationDetails, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required(if action.starts_with("discard_") {
            "discard working changes"
        } else {
            "delete branches"
        }));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    if action == "discard_file" || action == "discard_hunk" {
        return super::stage::prepare_discard_confirmation(
            runner,
            registry,
            repo_id,
            expected_version,
            action,
            targets,
        )
        .await;
    }
    if action != "branch_delete"
        && action != "merge_abort"
        && action != "conflict_accept"
        && action != "branch_move"
    {
        return Err(bad_request("Unsupported confirmation action"));
    }
    if action == "branch_move" {
        if targets.len() != 2 {
            return Err(bad_request(
                "Branch move confirms one branch and one commit",
            ));
        }
        let refs = list_refs(runner, &session).await?;
        let target = find_ref(&refs, &targets[0])?;
        let name = short_name(&target.full_name)
            .ok_or_else(|| bad_request("Only local branches can be moved"))?;
        let short_new: String = targets[1].chars().take(7).collect();
        let short_old: String = target.oid.chars().take(7).collect();
        let summary = format!(
            "Move branch '{name}' from {short_old} to {short_new}. Commits left behind stay reachable from the reflog for a while, but GitDock keeps no backup."
        );
        let (token, expires_at) =
            registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
        return Ok(ConfirmationDetails {
            confirmation_token: token,
            summary,
            expires_at,
        });
    }
    if action == "merge_abort" {
        if targets != ["merge"] {
            return Err(bad_request("Merge abort confirms the single live merge"));
        }
        let summary = super::merge::abort_summary(runner, registry, repo_id).await?;
        let (token, expires_at) =
            registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
        return Ok(ConfirmationDetails {
            confirmation_token: token,
            summary,
            expires_at,
        });
    }
    if action == "conflict_accept" {
        if targets.len() != 2 {
            return Err(bad_request(
                "Conflict accept confirms one file and one side",
            ));
        }
        let summary =
            super::merge::accept_summary(runner, registry, repo_id, &targets[0], &targets[1])
                .await?;
        let (token, expires_at) =
            registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
        return Ok(ConfirmationDetails {
            confirmation_token: token,
            summary,
            expires_at,
        });
    }
    if targets.is_empty() || targets.len() > 16 {
        return Err(bad_request("Select 1..=16 branches"));
    }
    let refs = list_refs(runner, &session).await?;
    let mut summaries: Vec<String> = Vec::new();
    for ref_id in targets {
        let target = find_ref(&refs, ref_id)?;
        let name = short_name(&target.full_name)
            .ok_or_else(|| bad_request("Only local branches can be deleted"))?;
        if target.current {
            return Err(bad_request("The current branch cannot be deleted"));
        }
        if target.checked_out_elsewhere {
            return Err(AppError::new(
                ErrorCode::REF_INVALID,
                format!("Branch '{name}' is checked out in another worktree"),
                RecoveryAction::InspectState,
                false,
            ));
        }
        let short_oid = target.oid.chars().take(12).collect::<String>();
        let merged = is_merged(runner, &session.worktree_root, &target.oid).await;
        summaries.push(format!(
            "Delete local branch '{name}' ({short_oid}); {}",
            if merged {
                "fully merged into HEAD"
            } else {
                "NOT merged into HEAD — safe delete will refuse"
            }
        ));
    }
    let (token, expires_at) =
        registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
    Ok(ConfirmationDetails {
        confirmation_token: token,
        summary: summaries.join("\n"),
        expires_at,
    })
}

async fn core_branch_delete(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("delete branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    registry.confirmation_consume(
        repo_id,
        session.version,
        "branch_delete",
        std::slice::from_ref(&ref_id.to_string()),
        confirmation_token,
    )?;
    let refs = list_refs(runner, &session).await?;
    let target = find_ref(&refs, ref_id)?;
    let name = short_name(&target.full_name)
        .ok_or_else(|| bad_request("Only local branches can be deleted"))?;
    if target.current {
        return Err(bad_request("The current branch cannot be deleted"));
    }
    if target.checked_out_elsewhere {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            format!("Branch '{name}' is checked out in another worktree"),
            RecoveryAction::InspectState,
            false,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    // Safe delete only: unmerged branches refuse here, never fall back to -D.
    let out = runner
        .run(
            &session.worktree_root,
            &["branch", "-d", name],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("delete the branch"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr);
        if detail.contains("not fully merged") {
            return Err(AppError::new(
                ErrorCode::REF_INVALID,
                "Branch is not merged; safe delete refuses. Merge it first.",
                RecoveryAction::InspectState,
                false,
            ));
        }
        return Err(git_failed("delete the branch"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

// ---- move / rename / upstream / push ----

/// Resolve a local branch ref or fail with a refreshable error.
async fn local_branch(
    runner: &GitRunner,
    session: &RepoSession,
    ref_id: &str,
) -> Result<crate::domain::RefItem, AppError> {
    let refs = list_refs(runner, session).await?;
    let target = find_ref(&refs, ref_id)?;
    if short_name(&target.full_name).is_none() {
        return Err(bad_request("Only local branches support this action"));
    }
    Ok(target)
}

async fn core_branch_move(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
    oid: &str,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("move branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    check_oid(&session.object_format, oid)?;
    if !object_exists(runner, &session.worktree_root, oid).await {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            "Target commit does not exist",
            RecoveryAction::Refresh,
            true,
        ));
    }
    registry.confirmation_consume(
        repo_id,
        session.version,
        "branch_move",
        &[ref_id.to_string(), oid.to_string()],
        confirmation_token,
    )?;
    let target = local_branch(runner, &session, ref_id).await?;
    let name = short_name(&target.full_name).expect("local branch");
    if target.current {
        return Err(bad_request(
            "The current branch cannot be moved; switch away first",
        ));
    }
    if target.checked_out_elsewhere {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            format!("Branch '{name}' is checked out in another worktree"),
            RecoveryAction::InspectState,
            false,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["branch", "-f", name, oid],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("move the branch"))?;
    if !out.success {
        return Err(git_failed("move the branch"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

async fn core_branch_rename(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
    new_name: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("rename branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.len() > 250 {
        return Err(bad_request("Branch name must be 1..=250 characters"));
    }
    validate_branch_name(runner, &session.worktree_root, new_name).await?;
    let target = local_branch(runner, &session, ref_id).await?;
    let old = short_name(&target.full_name).expect("local branch");
    if old == new_name {
        return Err(bad_request("The new name matches the current name"));
    }
    if target.checked_out_elsewhere {
        return Err(AppError::new(
            ErrorCode::REF_INVALID,
            format!("Branch '{old}' is checked out in another worktree"),
            RecoveryAction::InspectState,
            false,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &["branch", "-m", old, new_name],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("rename the branch"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr);
        if detail.contains("already exists") {
            return Err(bad_request("A branch with this name already exists"));
        }
        return Err(git_failed("rename the branch"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

async fn core_branch_set_upstream(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
    upstream_ref_id: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("set branch upstreams"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let target = local_branch(runner, &session, ref_id).await?;
    let name = short_name(&target.full_name).expect("local branch");
    let refs = list_refs(runner, &session).await?;
    let upstream = find_ref(&refs, upstream_ref_id)?;
    if upstream.kind != "remote" {
        return Err(bad_request("Upstream must be a remote tracking branch"));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let out = runner
        .run(
            &session.worktree_root,
            &[
                "branch",
                &format!("--set-upstream-to={}", upstream.full_name),
                name,
            ],
            WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("set the upstream"))?;
    if !out.success {
        return Err(git_failed("set the upstream"));
    }
    fresh_snapshot(runner, registry, repo_id).await
}

async fn core_branch_push(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    ref_id: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("push branches"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let target = local_branch(runner, &session, ref_id).await?;
    let name = short_name(&target.full_name).expect("local branch");

    // Resolve the tracked upstream first: pushes go only to the configured
    // destination, never force, never an invented refspec.
    let upstream = runner
        .run(
            &session.worktree_root,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                &format!("{name}@{{upstream}}"),
            ],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("read the upstream"))?;
    if !upstream.success {
        return Err(AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            "This branch has no upstream; use Set upstream first",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let upstream_ref = String::from_utf8_lossy(&upstream.stdout).trim().to_string();
    let mut parts = upstream_ref.splitn(2, '/');
    let remote = parts.next().unwrap_or("").to_string();
    let dest = parts.next().unwrap_or("").to_string();
    if remote.is_empty() || dest.is_empty() {
        return Err(AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            "This branch has no upstream; use Set upstream first",
            RecoveryAction::InspectState,
            false,
        ));
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let spec = format!("{name}:{dest}");
    let out = runner
        .run(
            &session.worktree_root,
            &["push", &remote, &spec],
            NETWORK_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed("push the branch"))?;
    if !out.success {
        let detail = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(push_terminal_error(&detail));
    }
    fresh_snapshot(runner, registry, repo_id).await
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
    git_failed("push the branch")
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchCreateRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub name: String,
    pub start_oid: String,
    pub switch_after_create: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSwitchRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
    pub new_local_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDeleteRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
    pub confirmation_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchMoveRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
    pub oid: String,
    pub confirmation_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRenameRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSetUpstreamRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
    pub upstream_ref_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPushRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub ref_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationPrepareRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub action: String,
    pub targets: Vec<String>,
}

// ---- Commands ----

#[tauri::command]
pub async fn branch_create(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchCreateRequest,
) -> Result<ApiResult<BranchCreateResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        if request.name.len() > 250 {
            return Err(bad_request("Branch name is too long"));
        }
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_create(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            request.name.trim(),
            &request.start_oid,
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
pub async fn branch_switch(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchSwitchRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_switch(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
            request.new_local_name.as_deref(),
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
pub async fn branch_delete(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchDeleteRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_delete(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
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
pub async fn branch_move(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchMoveRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_move(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
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
pub async fn branch_rename(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchRenameRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_rename(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
            request.new_name.trim(),
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
pub async fn branch_set_upstream(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchSetUpstreamRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_set_upstream(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
            &request.upstream_ref_id,
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
pub async fn branch_push(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BranchPushRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_branch_push(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.ref_id,
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
pub async fn confirmation_prepare(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: ConfirmationPrepareRequest,
) -> Result<ApiResult<ConfirmationDetails>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_confirmation_prepare(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.action,
            &request.targets,
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
        branch_create, branch_delete, branch_move, branch_push, branch_rename, branch_set_upstream,
        branch_switch, confirmation_prepare, BranchCreateRequest, BranchDeleteRequest,
        BranchMoveRequest, BranchPushRequest, BranchRenameRequest, BranchSetUpstreamRequest,
        BranchSwitchRequest, ConfirmationPrepareRequest,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::commit::tests::{git, open_repo, temp_repo};
    use crate::domain::TrustState;
    use std::process::Command as StdCommand;

    fn head_branch(repo: &std::path::Path) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()
            .expect("head");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn oid_of(repo: &std::path::Path, rev: &str) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["rev-parse", rev])
            .output()
            .expect("rev-parse");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    async fn prepare_move(
        runner: &GitRunner,
        registry: &mut RepoRegistry,
        repo_id: &str,
        version: u64,
        ref_id: &str,
        oid: &str,
    ) -> String {
        core_confirmation_prepare(
            runner,
            registry,
            repo_id,
            version,
            "branch_move",
            &[ref_id.to_string(), oid.to_string()],
        )
        .await
        .expect("prepare")
        .confirmation_token
    }

    #[tokio::test]
    async fn rename_moves_and_rejects_duplicates() {
        let (_dir, repo) = temp_repo("branches-rename");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let start = oid_of(&repo, "HEAD");
        core_branch_create(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "old-name",
            &start,
            false,
        )
        .await
        .expect("create");

        let version = registry.get(&repo_id).expect("session").version;
        core_branch_rename(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/old-name",
            "new-name",
        )
        .await
        .expect("rename");
        assert_eq!(head_branch(&repo), "main");
        let out = StdCommand::new("git")
            .current_dir(&repo)
            .args(["rev-parse", "--verify", "refs/heads/new-name"])
            .output()
            .expect("rev-parse");
        assert!(out.status.success());

        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_rename(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/new-name",
            "main",
        )
        .await
        .expect_err("duplicate");
        assert_eq!(err.code, crate::domain::ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn move_needs_token_and_repoints() {
        let (_dir, repo) = temp_repo("branches-move");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "first"]);
        git(&repo, &["commit", "--allow-empty", "-m", "second"]);
        let first = oid_of(&repo, "HEAD~1");
        let second = oid_of(&repo, "HEAD");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        core_branch_create(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "mover",
            &first,
            false,
        )
        .await
        .expect("create");

        // No token: fails closed.
        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_move(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/mover",
            &second,
            "bogus",
        )
        .await
        .expect_err("token required");
        assert_eq!(err.code, crate::domain::ErrorCode::INVALID_ARGUMENT);

        let version = registry.get(&repo_id).expect("session").version;
        let token = prepare_move(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/mover",
            &second,
        )
        .await;
        let version = registry.get(&repo_id).expect("session").version;
        core_branch_move(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/mover",
            &second,
            &token,
        )
        .await
        .expect("move");
        assert_eq!(oid_of(&repo, "mover"), second);
    }

    #[tokio::test]
    async fn upstream_and_push_round_trip_over_local_remote() {
        let (_dir, repo) = temp_repo("branches-push");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        let bare = repo.parent().expect("parent").join("remote.git");
        git(&repo, &["init", "--bare", bare.to_str().expect("path")]);
        git(
            &repo,
            &["remote", "add", "origin", bare.to_str().expect("path")],
        );
        git(&repo, &["push", "origin", "main"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);

        // No upstream yet: push refuses with guidance.
        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_push(&runner, &mut registry, &repo_id, version, "refs/heads/main")
            .await
            .expect_err("no upstream");
        assert_eq!(err.code, crate::domain::ErrorCode::INVALID_ARGUMENT);

        // Non-remote upstream refuses.
        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_set_upstream(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/main",
            "refs/heads/main",
        )
        .await
        .expect_err("remote only");
        assert_eq!(err.code, crate::domain::ErrorCode::INVALID_ARGUMENT);

        // Point main at origin/main, then push a new commit through the app.
        git(&repo, &["fetch", "origin"]);
        let version = registry.get(&repo_id).expect("session").version;
        core_branch_set_upstream(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "refs/heads/main",
            "refs/remotes/origin/main",
        )
        .await
        .expect("set upstream");
        git(&repo, &["commit", "--allow-empty", "-m", "second"]);
        let version = registry.get(&repo_id).expect("session").version;
        core_branch_push(&runner, &mut registry, &repo_id, version, "refs/heads/main")
            .await
            .expect("push");
        assert_eq!(oid_of(&repo, "main"), oid_of(&bare, "main"));
    }

    async fn prepare_delete(
        runner: &GitRunner,
        registry: &mut RepoRegistry,
        repo_id: &str,
        version: u64,
        ref_id: &str,
    ) -> ConfirmationDetails {
        core_confirmation_prepare(
            runner,
            registry,
            repo_id,
            version,
            "branch_delete",
            &[ref_id.to_string()],
        )
        .await
        .expect("prepare")
    }

    #[tokio::test]
    async fn create_switch_and_delete_round_trip() {
        let (_dir, repo) = temp_repo("branches");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let start = oid_of(&repo, "HEAD");

        // Create without switching: HEAD stays, branch points at start.
        let created = core_branch_create(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "feature",
            &start,
            false,
        )
        .await
        .expect("create");
        assert!(!created.switched);
        assert_eq!(head_branch(&repo), "main");

        // Switch cleanly to the new branch.
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        let feature = refs
            .iter()
            .find(|r| r.full_name == "refs/heads/feature")
            .expect("feature ref")
            .clone();
        core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            created.snapshot.version,
            &feature.ref_id,
            None,
        )
        .await
        .expect("switch");
        assert_eq!(head_branch(&repo), "feature");

        // Delete the merged branch behind a confirmation token.
        let main_ref = refs
            .iter()
            .find(|r| r.full_name == "refs/heads/main")
            .expect("main ref")
            .ref_id
            .clone();
        let back_version = registry.get(&repo_id).expect("session").version;
        core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            back_version,
            &main_ref,
            None,
        )
        .await
        .expect("switch back");
        let version = registry.get(&repo_id).expect("session").version;
        let confirm =
            prepare_delete(&runner, &mut registry, &repo_id, version, &feature.ref_id).await;
        assert!(confirm.summary.contains("fully merged"));
        core_branch_delete(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &feature.ref_id,
            &confirm.confirmation_token,
        )
        .await
        .expect("delete");
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        assert!(refs.iter().all(|r| r.full_name != "refs/heads/feature"));

        // The token is single-use: replaying it fails closed.
        let replay_version = registry.get(&repo_id).expect("session").version;
        let replay = core_branch_delete(
            &runner,
            &mut registry,
            &repo_id,
            replay_version,
            &feature.ref_id,
            &confirm.confirmation_token,
        )
        .await;
        assert!(matches!(
            replay,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));
    }

    #[tokio::test]
    async fn remote_checkout_creates_tracking_branch_and_switches() {
        let (_dir, repo) = temp_repo("branches-remote-checkout");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        let bare = repo.parent().expect("parent").join("remote.git");
        git(&repo, &["init", "--bare", bare.to_str().expect("path")]);
        git(
            &repo,
            &["remote", "add", "origin", bare.to_str().expect("path")],
        );
        // Publish a branch that only exists on the remote side.
        git(&repo, &["checkout", "-b", "feature"]);
        git(&repo, &["commit", "--allow-empty", "-m", "feature work"]);
        git(&repo, &["push", "origin", "feature"]);
        git(&repo, &["checkout", "main"]);
        git(&repo, &["branch", "-D", "feature"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        let remote = refs
            .iter()
            .find(|r| r.full_name == "refs/remotes/origin/feature")
            .expect("remote feature ref")
            .clone();

        // Checkout with an explicit local name: `git switch -c` semantics.
        let version = registry.get(&repo_id).expect("session").version;
        core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &remote.ref_id,
            Some("feature"),
        )
        .await
        .expect("remote checkout");
        assert_eq!(head_branch(&repo), "feature");
        assert_eq!(oid_of(&repo, "feature"), oid_of(&repo, "origin/feature"));
        assert_eq!(
            oid_of(&repo, "feature@{upstream}"),
            oid_of(&repo, "origin/feature")
        );

        // A dirty worktree refuses so the UI can offer stash-and-switch.
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("dirty.txt"), "uncommitted").expect("dirty file");
        git(&repo, &["push", "origin", "main:other"]);
        git(&repo, &["fetch", "origin"]);
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        let other = refs
            .iter()
            .find(|r| r.full_name == "refs/remotes/origin/other")
            .expect("remote other ref")
            .clone();
        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &other.ref_id,
            Some("other"),
        )
        .await
        .expect_err("dirty remote checkout");
        assert_eq!(err.code, crate::domain::ErrorCode::DIRTY_WORKTREE);
        assert_eq!(head_branch(&repo), "main");

        // A tracking name that already exists locally fails closed: git
        // refuses `switch -c` and HEAD never moves. The UI checks this first
        // and asks for another name instead.
        std::fs::remove_file(repo.join("dirty.txt")).expect("clean up");
        let version = registry.get(&repo_id).expect("session").version;
        let err = core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &other.ref_id,
            Some("main"),
        )
        .await
        .expect_err("duplicate tracking name");
        assert_eq!(err.code, crate::domain::ErrorCode::GIT_ERROR);
        assert_eq!(head_branch(&repo), "main");
    }

    #[tokio::test]
    async fn dirty_switch_and_unmerged_delete_refuse() {
        let (_dir, repo) = temp_repo("refusals");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        git(&repo, &["checkout", "-b", "side"]);
        std::fs::write(repo.join("side.txt"), "side\n").expect("write");
        git(&repo, &["add", "side.txt"]);
        git(&repo, &["commit", "-m", "side"]);
        git(&repo, &["checkout", "main"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);

        // Dirty worktree blocks switching.
        std::fs::write(repo.join("dirty.txt"), "dirty\n").expect("write");
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        let side_id = refs
            .iter()
            .find(|r| r.full_name == "refs/heads/side")
            .expect("side")
            .ref_id
            .clone();
        let dirty = core_branch_switch(
            &runner,
            &mut registry,
            &repo_id,
            session.version,
            &side_id,
            None,
        )
        .await;
        assert!(matches!(
            dirty,
            Err(ref e) if e.code == ErrorCode::DIRTY_WORKTREE
        ));
        assert_eq!(head_branch(&repo), "main");
        std::fs::remove_file(repo.join("dirty.txt")).expect("clean");

        // Unmerged branches refuse safe delete (never -D behind our back).
        let version = registry.get(&repo_id).expect("session").version;
        let confirm = prepare_delete(&runner, &mut registry, &repo_id, version, &side_id).await;
        assert!(confirm.summary.contains("NOT merged"));
        let refused = core_branch_delete(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &side_id,
            &confirm.confirmation_token,
        )
        .await;
        assert!(matches!(
            refused,
            Err(ref e) if e.code == ErrorCode::REF_INVALID
        ));
        // The branch is still there.
        let session = registry.get(&repo_id).expect("session").clone();
        let refs = list_refs(&runner, &session).await.expect("refs");
        assert!(refs.iter().any(|r| r.full_name == "refs/heads/side"));

        // The current branch is refused already at prepare time.
        let main_id = refs
            .iter()
            .find(|r| r.full_name == "refs/heads/main")
            .expect("main")
            .ref_id
            .clone();
        let version = registry.get(&repo_id).expect("session").version;
        let current = core_confirmation_prepare(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "branch_delete",
            &[main_id],
        )
        .await;
        assert!(matches!(
            current,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));
    }

    #[tokio::test]
    async fn create_then_dirty_switch_reports_partial_success() {
        let (_dir, repo) = temp_repo("partial");
        git(&repo, &["config", "user.name", "T Ten"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("dirty.txt"), "dirty\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let start = oid_of(&repo, "HEAD");

        // Create succeeds, the dirty switch does not: explicit partial flags.
        let result = core_branch_create(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "staged-branch",
            &start,
            true,
        )
        .await
        .expect("create returns");
        assert!(!result.switched);
        assert!(matches!(
            result.switch_error,
            Some(ref e) if e.code == ErrorCode::DIRTY_WORKTREE
        ));
        assert_eq!(head_branch(&repo), "main");
        let out = StdCommand::new("git")
            .current_dir(&repo)
            .args(["rev-parse", "--verify", "refs/heads/staged-branch"])
            .output()
            .expect("verify");
        assert!(out.status.success());
    }
}
