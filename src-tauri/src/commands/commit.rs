//! `identity_read` and `commit_create` (T10).
//!
//! Commits take exactly the index — never worktree bytes — with the message
//! passed via stdin. A failed commit never clears the UI draft: the draft
//! lives client-side and every failure keeps it intact by construction.

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, CommitResult, ErrorCode, IdentityInfo, RecoveryAction, RequestId,
    TrustState,
};
use crate::git::{
    commit_staged, has_unmerged, index_is_empty, read_identity, CommitError, GitRunner,
};
use crate::services::RepoRegistry;

const MAX_SUBJECT_CHARS: usize = 500;
const MAX_BODY_BYTES: usize = 64 * 1024;

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
        "Trust this repository to commit",
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

fn commit_error(error: CommitError) -> AppError {
    match error {
        CommitError::Run(run) => match run {
            crate::git::runner::RunError::TimedOut => AppError::new(
                ErrorCode::TIMEOUT,
                "The commit timed out; inspect history before retrying",
                RecoveryAction::InspectState,
                false,
            ),
            crate::git::runner::RunError::OutputLimit => AppError::new(
                ErrorCode::OUTPUT_LIMIT,
                "Commit message exceeds the size limit",
                RecoveryAction::InspectState,
                false,
            ),
            crate::git::runner::RunError::SpawnFailed(_) => AppError::new(
                ErrorCode::GIT_ERROR,
                "Failed to run the commit",
                RecoveryAction::Refresh,
                true,
            ),
        },
        CommitError::GitFailed(stderr) => {
            // Never auto-retry a mutation; classify for guidance instead.
            let lowered = stderr.to_lowercase();
            if lowered.contains("gpg") || lowered.contains("signing") || lowered.contains("sign") {
                AppError::new(
                    ErrorCode::SIGNING_FAILED,
                    "Commit signing failed; your draft is kept. Check your signing setup, then retry.",
                    RecoveryAction::InspectState,
                    false,
                )
            } else if lowered.contains("hook") {
                AppError::new(
                    ErrorCode::HOOK_FAILED,
                    "A Git hook refused the commit; your draft is kept. Read the hook output, fix it, then retry.",
                    RecoveryAction::InspectState,
                    false,
                )
            } else {
                AppError::new(
                    ErrorCode::GIT_ERROR,
                    "The commit failed; your draft is kept.",
                    RecoveryAction::InspectState,
                    false,
                )
            }
        }
    }
}

async fn core_identity(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
) -> Result<IdentityInfo, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let identity = read_identity(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?;
    Ok(IdentityInfo {
        name: identity.name,
        email: identity.email,
        scope: identity.scope,
        signing: identity.signing,
    })
}

async fn core_commit(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    subject: &str,
    body: &str,
) -> Result<CommitResult, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let subject = subject.trim();
    if subject.is_empty() {
        return Err(bad_request("Commit subject cannot be empty"));
    }
    if subject.chars().count() > MAX_SUBJECT_CHARS {
        return Err(bad_request("Commit subject exceeds 500 characters"));
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(bad_request("Commit body exceeds 64 KiB"));
    }
    if has_unmerged(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?
    {
        return Err(AppError::new(
            ErrorCode::CONFLICTS_PRESENT,
            "Unresolved conflicts block the commit; resolve them first",
            RecoveryAction::ResolveConflict,
            false,
        ));
    }
    if index_is_empty(runner, &session.worktree_root)
        .await
        .map_err(commit_error)?
    {
        return Err(AppError::new(
            ErrorCode::EMPTY_INDEX,
            "Nothing is staged; stage files before committing",
            RecoveryAction::InspectState,
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

    // 72 chars is a soft guideline, never a hard reject (engine §4).
    let mut message = String::with_capacity(subject.len() + body.len() + 4);
    message.push_str(subject);
    let body = body.trim();
    if !body.is_empty() {
        message.push_str("\n\n");
        message.push_str(body);
    }
    message.push('\n');
    let oid = commit_staged(runner, &session.worktree_root, message.as_bytes())
        .await
        .map_err(commit_error)?;
    registry.bump(repo_id).ok_or_else(session_missing)?;
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let snapshot = super::repos::build_snapshot(runner, registry, &session).await?;
    Ok(CommitResult {
        oid,
        snapshot: Box::new(snapshot),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityRequest {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitCreateRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub subject: String,
    #[serde(default)]
    pub body: String,
}

#[tauri::command]
pub async fn identity_read(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: IdentityRequest,
) -> Result<ApiResult<IdentityInfo>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        core_identity(&runner, &registry, &request.repo_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn commit_create(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: CommitCreateRequest,
) -> Result<ApiResult<CommitResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_commit(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
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

pub mod prelude {
    pub use super::{commit_create, identity_read, CommitCreateRequest, IdentityRequest};
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::TrustState;
    use std::process::Command as StdCommand;

    pub(crate) fn git(cwd: &std::path::Path, args: &[&str]) {
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

    pub(crate) fn temp_repo(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("gitdock-t10-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        // Commands exercised through GitRunner do not inherit the helper's
        // one-shot identity environment, so make each fixture self-contained.
        git(&repo, &["config", "user.name", "Octopus Test"]);
        git(&repo, &["config", "user.email", "octopus-test@example.com"]);
        (dir, repo)
    }

    pub(crate) fn open_repo(
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

    fn with_identity(repo: &std::path::Path) {
        git(repo, &["config", "user.name", "T Ten"]);
        git(repo, &["config", "user.email", "t@example.com"]);
    }

    /// Blank repo-local identity shadows any global config, simulating a
    /// machine with no identity anywhere — without touching process-wide
    /// env that sibling tests rely on for their own commits.
    fn blank_identity(repo: &std::path::Path) {
        git(repo, &["config", "user.name", ""]);
        git(repo, &["config", "user.email", ""]);
    }

    #[tokio::test]
    async fn daily_commit_e2e_commits_only_the_index() {
        let (_dir, repo) = temp_repo("daily");
        with_identity(&repo);
        // Two files changed, only one staged: the commit takes the index.
        std::fs::write(repo.join("staged.txt"), "s\n").expect("write");
        std::fs::write(repo.join("unstaged.txt"), "u\n").expect("write");
        git(&repo, &["add", "staged.txt"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;

        let result = core_commit(&runner, &mut registry, &repo_id, version, "Daily work", "")
            .await
            .expect("commit");
        assert_eq!(result.oid.len(), 40);
        // The new commit is visible in history; the unstaged file survived.
        let tips = vec![result.oid.clone()];
        let (topo, truncated) =
            crate::git::read_topology(&runner, registry.get(&repo_id).unwrap(), &tips)
                .await
                .expect("topology");
        assert!(!truncated);
        assert!(topo.iter().any(|row| row.oid == result.oid));
        assert_eq!(
            std::fs::read(repo.join("unstaged.txt")).expect("read"),
            b"u\n"
        );
        // Index is clean again after the commit.
        assert!(result.snapshot.staged_count == Some(0));
    }

    #[tokio::test]
    async fn unborn_first_commit_and_empty_subject_blocked() {
        let (_dir, repo) = temp_repo("unborn-commit");
        with_identity(&repo);
        std::fs::write(repo.join("first.txt"), "hello\n").expect("write");
        git(&repo, &["add", "first.txt"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;

        let empty = core_commit(&runner, &mut registry, &repo_id, version, "   ", "").await;
        assert!(matches!(
            empty,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));

        let result = core_commit(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "First commit",
            "body text",
        )
        .await
        .expect("first commit");
        assert_eq!(result.oid.len(), 40);
        // HEAD now exists on main.
        let head = StdCommand::new("git")
            .current_dir(&repo)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()
            .expect("head");
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "main");
    }

    #[tokio::test]
    async fn empty_index_identity_and_conflicts_block() {
        let (_dir, repo) = temp_repo("blocks");
        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;

        // Something staged but blank local identity shadowing any global
        // config: reads as missing without touching sibling tests' env.
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        blank_identity(&repo);
        let missing = core_commit(&runner, &mut registry, &repo_id, version, "x", "").await;
        assert!(matches!(
            missing,
            Err(ref e) if e.code == ErrorCode::IDENTITY_MISSING
        ));

        with_identity(&repo);
        // Identity fixed, index emptied again: the other guard fires.
        git(&repo, &["rm", "--cached", "a.txt"]);
        let empty = core_commit(&runner, &mut registry, &repo_id, version, "x", "").await;
        assert!(matches!(
            empty,
            Err(ref e) if e.code == ErrorCode::EMPTY_INDEX
        ));
    }

    #[tokio::test]
    async fn hook_failure_keeps_everything_for_retry() {
        let (_dir, repo) = temp_repo("hook");
        with_identity(&repo);
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        // A pre-commit hook that always refuses.
        std::fs::create_dir_all(repo.join(".git").join("hooks")).expect("hooks");
        std::fs::write(
            repo.join(".git").join("hooks").join("pre-commit"),
            "#!/bin/sh\necho hook says no >&2\nexit 1\n",
        )
        .expect("hook");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(repo.join(".git").join("hooks").join("pre-commit"))
                .expect("meta")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(repo.join(".git").join("hooks").join("pre-commit"), perms)
                .expect("chmod");
        }

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;

        let failed = core_commit(&runner, &mut registry, &repo_id, version, "try", "").await;
        assert!(matches!(
            failed,
            Err(ref e) if e.code == ErrorCode::HOOK_FAILED
        ));
        // Nothing committed, index untouched: retry is meaningful.
        let head = StdCommand::new("git")
            .current_dir(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("head");
        assert!(!head.status.success());
        let staged = StdCommand::new("git")
            .current_dir(&repo)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .expect("staged");
        assert_eq!(String::from_utf8_lossy(&staged.stdout).trim(), "a.txt");
    }

    #[tokio::test]
    async fn detached_head_commit_is_allowed() {
        let (_dir, repo) = temp_repo("detached");
        with_identity(&repo);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        git(&repo, &["checkout", "--detach", "HEAD"]);
        std::fs::write(repo.join("d.txt"), "d\n").expect("write");
        git(&repo, &["add", "d.txt"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;

        let result = core_commit(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "detached work",
            "",
        )
        .await
        .expect("detached commit");
        assert_eq!(result.oid.len(), 40);
    }
}
