//! `index_stage` / `index_unstage`: exact-path index mutations (T09).
//!
//! Both commands take a `WriteContext` (`expectedVersion`) and a non-empty
//! list of path ids resolved against the CURRENT status listing — never
//! display strings, never `add .`. Flow per mutation:
//!
//! 1. Trust gate (trusted only), version check, token resolution.
//! 2. Serialize on the per-common-dir write queue (linked worktrees share it).
//! 3. Run one Git invocation with `:(literal)` raw-byte argv (no shell).
//! 4. Rescan, bump the snapshot version, return a fresh snapshot.
//!
//! Failures also bump the version and refresh the listing: after an
//! uncertain write the old tokens must not look valid. The UI keeps its
//! file selection on errors and reloads the listing for fresh tokens.

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, ConfirmationDetails, ErrorCode, RecoveryAction, RepoSnapshot, RequestId,
};
use crate::git::{
    patch_fingerprint, read_worktree_patch, select_patch_hunk, DiffError, GitRunner, READ_TIMEOUT,
};
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
        "Trust this repository to stage or unstage files",
        RecoveryAction::InspectState,
        false,
    )
}

fn stale_state() -> AppError {
    AppError::new(
        ErrorCode::STALE_STATE,
        "The repository changed under you; refresh working changes and retry",
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
        "Git could not update the index for this selection",
        RecoveryAction::Refresh,
        true,
    )
}

fn hunk_error(error: DiffError) -> AppError {
    match error {
        DiffError::Invalid(message) => AppError::new(
            ErrorCode::STALE_STATE,
            message,
            RecoveryAction::Refresh,
            false,
        ),
        DiffError::TooLarge | DiffError::Run(crate::git::RunError::OutputLimit) => AppError::new(
            ErrorCode::OUTPUT_LIMIT,
            "This diff is too large for a partial mutation",
            RecoveryAction::InspectState,
            false,
        ),
        DiffError::Run(crate::git::RunError::TimedOut) => AppError::new(
            ErrorCode::TIMEOUT,
            "Reading the current hunk timed out",
            RecoveryAction::RetryRead,
            true,
        ),
        DiffError::Run(crate::git::RunError::SpawnFailed(_)) | DiffError::GitFailed(_) => {
            AppError::new(
                ErrorCode::GIT_ERROR,
                "Git could not rebuild the current hunk",
                RecoveryAction::Refresh,
                true,
            )
        }
    }
}

/// Raw argv with one `:(literal)`-prefixed path per entry. The prefix is
/// ASCII so non-UTF-8 paths stay byte-exact (same trick as the diff engine).
fn argv_with_paths(base: &[&str], paths: &[Vec<u8>]) -> Vec<std::ffi::OsString> {
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::OsStrExt;
    let mut argv: Vec<OsString> = base.iter().map(OsString::from).collect();
    argv.push(OsString::from("--"));
    for raw in paths {
        let mut literal = b":(literal)".to_vec();
        literal.extend_from_slice(raw);
        argv.push(OsStr::from_bytes(&literal).to_os_string());
    }
    argv
}

async fn path_is_tracked(
    runner: &GitRunner,
    cwd: &std::path::Path,
    raw_path: &[u8],
) -> Result<bool, AppError> {
    let out = runner
        .run(
            cwd,
            &argv_with_paths(&["ls-files", "--error-unmatch"], &[raw_path.to_vec()]),
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed())?;
    Ok(out.success)
}

/// Bind whole-file discard confirmation to the current bytes. Tracked files
/// use Git's binary patch (including blob ids); untracked files use the blob
/// id Git would write. Paths are backend-resolved raw bytes in both cases.
async fn discard_fingerprint(
    runner: &GitRunner,
    cwd: &std::path::Path,
    raw_path: &[u8],
    tracked: bool,
) -> Result<String, AppError> {
    if !tracked {
        use std::os::unix::ffi::OsStrExt;
        let full = cwd.join(std::ffi::OsStr::from_bytes(raw_path));
        let metadata = std::fs::symlink_metadata(&full).map_err(|_| {
            AppError::new(
                ErrorCode::STALE_STATE,
                "The file changed or no longer exists; refresh working changes",
                RecoveryAction::Refresh,
                false,
            )
        })?;
        if metadata.file_type().is_symlink() {
            let target = std::fs::read_link(&full).map_err(|_| stale_state())?;
            return Ok(patch_fingerprint(target.as_os_str().as_bytes()));
        }
        if !metadata.file_type().is_file() {
            return Err(bad_request(
                "Discard supports individual files and symbolic links only",
            ));
        }
    }
    let out = if tracked {
        runner
            .run(
                cwd,
                &argv_with_paths(
                    &[
                        "diff",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--no-color",
                        "--binary",
                    ],
                    &[raw_path.to_vec()],
                ),
                READ_TIMEOUT,
            )
            .await
    } else {
        use std::ffi::{OsStr, OsString};
        use std::os::unix::ffi::OsStrExt;
        let argv = vec![
            OsString::from("hash-object"),
            OsString::from("--no-filters"),
            OsString::from("--"),
            OsStr::from_bytes(raw_path).to_os_string(),
        ];
        runner.run(cwd, &argv, READ_TIMEOUT).await
    }
    .map_err(|_| git_failed())?;
    if !out.success {
        return Err(AppError::new(
            ErrorCode::STALE_STATE,
            "The file changed or no longer exists; refresh working changes",
            RecoveryAction::Refresh,
            false,
        ));
    }
    Ok(patch_fingerprint(&out.stdout))
}

/// Build destructive-action copy and a single-use token. This is called by
/// the shared `confirmation_prepare` command so all confirmations have one
/// IPC entry point.
pub(crate) async fn prepare_discard_confirmation(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    action: &str,
    targets: &[String],
) -> Result<ConfirmationDetails, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != crate::domain::TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    match action {
        "discard_file" if targets.len() == 1 => {
            let (raw, _) = registry.status_resolve(repo_id, &targets[0])?;
            let tracked = path_is_tracked(runner, &session.worktree_root, &raw).await?;
            let fingerprint =
                discard_fingerprint(runner, &session.worktree_root, &raw, tracked).await?;
            let binding = vec![targets[0].clone(), fingerprint];
            let (token, expires_at) =
                registry.confirmation_issue(repo_id, session.version, action, binding);
            let path = String::from_utf8_lossy(&raw);
            Ok(ConfirmationDetails {
                confirmation_token: token,
                summary: if tracked {
                    format!("Discard all unstaged changes in '{path}' and restore its index version. This cannot be undone by Octopus.")
                } else {
                    format!("Delete untracked file '{path}'. This cannot be undone by Octopus.")
                },
                expires_at,
            })
        }
        "discard_hunk" if targets.len() == 2 => {
            let (raw, _) = registry.status_resolve(repo_id, &targets[0])?;
            let patch = read_worktree_patch(runner, &session.worktree_root, &raw)
                .await
                .map_err(hunk_error)?;
            select_patch_hunk(&patch, &targets[1]).map_err(hunk_error)?;
            let (token, expires_at) =
                registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
            Ok(ConfirmationDetails {
                confirmation_token: token,
                summary: format!(
                    "Discard this hunk from '{}'. The selected lines will be restored from the index and cannot be recovered by Octopus.",
                    String::from_utf8_lossy(&raw)
                ),
                expires_at,
            })
        }
        "history_reset_hard"
        | "history_rebase"
        | "history_rebase_interactive"
        | "history_split"
            if targets.len() == 1 =>
        {
            let short = targets[0].chars().take(7).collect::<String>();
            let summary = match action {
                "history_reset_hard" => format!(
                    "Hard-reset the current branch to {short}. Index and worktree files change to match; uncommitted work is destroyed and cannot be recovered by Octopus."
                ),
                "history_rebase" => format!(
                    "Replay the current branch on top of {short}. Commits are rewritten with new IDs; resolve any conflicts in a terminal."
                ),
                "history_rebase_interactive" => format!(
                    "Rewrite history starting at {short} using your plan. Drops and squashes are permanent without a backup outside Octopus."
                ),
                _ => format!(
                    "Split {short}: uncommit HEAD into the worktree so its changes can be re-committed in parts. The original commit ID disappears."
                ),
            };
            let (token, expires_at) =
                registry.confirmation_issue(repo_id, session.version, action, targets.to_vec());
            Ok(ConfirmationDetails {
                confirmation_token: token,
                summary,
                expires_at,
            })
        }
        _ => Err(bad_request("Invalid discard confirmation target")),
    }
}

async fn head_exists(runner: &GitRunner, cwd: &std::path::Path) -> Result<bool, AppError> {
    runner
        .run(cwd, &["rev-parse", "HEAD"], READ_TIMEOUT)
        .await
        .map(|out| out.success)
        .map_err(|_| git_failed())
}

async fn core_mutate(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    path_ids: &[String],
    staged: bool,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != crate::domain::TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    if path_ids.is_empty() {
        return Err(bad_request("Select at least one file"));
    }
    // Resolve every token BEFORE touching the queue: all-or-nothing input.
    let mut raw_paths: Vec<Vec<u8>> = Vec::with_capacity(path_ids.len());
    for id in path_ids {
        let (raw, orig) = registry.status_resolve(repo_id, id)?;
        // Rename pairs (`2` records) only exist once staged. Staging the new
        // side is enough (adding the gone old side would fail the whole
        // invocation); unstaging moves both sides so no half rename lingers
        // in the index.
        if !staged {
            if let Some(old) = orig {
                if !raw_paths.contains(&old) {
                    raw_paths.push(old);
                }
            }
        }
        if !raw_paths.contains(&raw) {
            raw_paths.push(raw);
        }
    }

    // Serialize mutations of one common dir (linked worktrees included).
    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;

    let argv = if staged {
        argv_with_paths(&["add"], &raw_paths)
    } else if head_exists(runner, &session.worktree_root).await? {
        // Index-only restore from HEAD: worktree bytes are never touched.
        argv_with_paths(&["restore", "--source=HEAD", "--staged"], &raw_paths)
    } else {
        // Unborn HEAD: drop the entries from the index, keep the files.
        argv_with_paths(&["rm", "--cached"], &raw_paths)
    };
    // Plain `run`: GIT_OPTIONAL_LOCKS=0 is a read-only optimization and
    // must not be used for mutations.
    let out = runner
        .run(&session.worktree_root, &argv, crate::git::WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed())?;
    // The snapshot rebuild re-reads status (fresh listing, new generation);
    // bump first so the version reflects the attempted write either way.
    registry.bump(repo_id).ok_or_else(session_missing)?;
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if !out.success {
        let _ = super::repos::build_snapshot(runner, registry, &session).await;
        return Err(git_failed());
    }
    super::repos::build_snapshot(runner, registry, &session).await
}

async fn core_hunk_mutate(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    path_id: &str,
    hunk_id: &str,
    discard_confirmation: Option<&str>,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != crate::domain::TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    if hunk_id.len() > 64 || !hunk_id.starts_with("h1:") {
        return Err(bad_request("Malformed hunk id"));
    }
    let (raw, _) = registry.status_resolve(repo_id, path_id)?;
    let patch = read_worktree_patch(runner, &session.worktree_root, &raw)
        .await
        .map_err(hunk_error)?;
    let selected = select_patch_hunk(&patch, hunk_id).map_err(hunk_error)?;
    if let Some(token) = discard_confirmation {
        registry.confirmation_consume(
            repo_id,
            session.version,
            "discard_hunk",
            &[path_id.to_string(), hunk_id.to_string()],
            token,
        )?;
    }

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;
    let argv: &[&str] = if discard_confirmation.is_some() {
        &["apply", "--reverse", "--recount", "--whitespace=nowarn"]
    } else {
        &["apply", "--cached", "--recount", "--whitespace=nowarn"]
    };
    let out = runner
        .run_with_stdin(
            &session.worktree_root,
            argv,
            &selected,
            crate::git::WRITE_TIMEOUT,
        )
        .await
        .map_err(|_| git_failed())?;
    registry.bump(repo_id).ok_or_else(session_missing)?;
    let fresh = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if !out.success {
        let _ = super::repos::build_snapshot(runner, registry, &fresh).await;
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            if discard_confirmation.is_some() {
                "Git could not discard this hunk; refresh the diff and retry"
            } else {
                "Git could not stage this hunk; refresh the diff and retry"
            },
            RecoveryAction::Refresh,
            true,
        ));
    }
    super::repos::build_snapshot(runner, registry, &fresh).await
}

async fn core_discard_file(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    expected_version: u64,
    path_id: &str,
    confirmation_token: &str,
) -> Result<RepoSnapshot, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != crate::domain::TrustState::Trusted {
        return Err(trust_required());
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let (raw, _) = registry.status_resolve(repo_id, path_id)?;
    let tracked = path_is_tracked(runner, &session.worktree_root, &raw).await?;
    let fingerprint = discard_fingerprint(runner, &session.worktree_root, &raw, tracked).await?;
    registry.confirmation_consume(
        repo_id,
        session.version,
        "discard_file",
        &[path_id.to_string(), fingerprint],
        confirmation_token,
    )?;

    let queue = registry.queue_for(&session.key());
    let _guard = queue.lock().await;
    let argv = if tracked {
        argv_with_paths(&["restore", "--worktree"], &[raw])
    } else {
        // Exact path only, no `-d` and no recursive directory deletion.
        argv_with_paths(&["clean", "-f"], &[raw])
    };
    let out = runner
        .run(&session.worktree_root, &argv, crate::git::WRITE_TIMEOUT)
        .await
        .map_err(|_| git_failed())?;
    registry.bump(repo_id).ok_or_else(session_missing)?;
    let fresh = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if !out.success {
        let _ = super::repos::build_snapshot(runner, registry, &fresh).await;
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            "Git could not discard this file; refresh working changes and retry",
            RecoveryAction::Refresh,
            true,
        ));
    }
    super::repos::build_snapshot(runner, registry, &fresh).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMutationRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkMutationRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub hunk_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardHunkRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub hunk_id: String,
    pub confirmation_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardFileRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub path_id: String,
    pub confirmation_token: String,
}

async fn handle(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: IndexMutationRequest,
    staged: bool,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        if request.path_ids.len() > 1000 {
            return Err(bad_request("Too many paths in one request"));
        }
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_mutate(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.path_ids,
            staged,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => ApiResult::ok(snapshot, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn index_stage(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: IndexMutationRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    handle(registry, request, true).await
}

#[tauri::command]
pub async fn index_unstage(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: IndexMutationRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    handle(registry, request, false).await
}

#[tauri::command]
pub async fn diff_hunk_stage(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HunkMutationRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_hunk_mutate(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.path_id,
            &request.hunk_id,
            None,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => ApiResult::ok(snapshot, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn diff_hunk_discard(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: DiscardHunkRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_hunk_mutate(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.path_id,
            &request.hunk_id,
            Some(&request.confirmation_token),
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => ApiResult::ok(snapshot, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn worktree_discard_file(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: DiscardFileRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_discard_file(
            &runner,
            &mut registry,
            &request.repo_id,
            request.expected_version,
            &request.path_id,
            &request.confirmation_token,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => ApiResult::ok(snapshot, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

pub mod prelude {
    pub use super::{
        diff_hunk_discard, diff_hunk_stage, index_stage, index_unstage, worktree_discard_file,
        DiscardFileRequest, DiscardHunkRequest, HunkMutationRequest, IndexMutationRequest,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TrustState;
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
        let dir = std::env::temp_dir().join(format!("gitdock-t09-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        (dir, repo)
    }

    fn open_repo(registry: &mut RepoRegistry, repo: &std::path::Path, trust: TrustState) -> String {
        let discovered = crate::git::DiscoveredRepo {
            worktree_root: repo.to_path_buf(),
            git_dir: repo.join(".git"),
            common_dir: repo.join(".git"),
            object_format: "sha1".to_string(),
            bare: false,
        };
        registry.open(&discovered, trust).repo_id
    }

    /// Current path ids via a fresh status read, exactly like the UI.
    async fn live_ids(
        runner: &GitRunner,
        registry: &mut RepoRegistry,
        repo_id: &str,
    ) -> Vec<(String, String)> {
        let session = registry.get(repo_id).expect("session").clone();
        let parsed = crate::git::read_status(runner, &session.worktree_root)
            .await
            .expect("status");
        let raws: Vec<Vec<u8>> = parsed.files.iter().map(|f| f.path.clone()).collect();
        let origs: Vec<Option<Vec<u8>>> =
            parsed.files.iter().map(|f| f.orig_path.clone()).collect();
        registry.status_put(repo_id, raws, origs);
        parsed
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    String::from_utf8_lossy(&f.path).into_owned(),
                    registry.status_path_id(repo_id, i).expect("token"),
                )
            })
            .collect()
    }

    fn staged_entries(repo: &std::path::Path) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["ls-files", "--stage"])
            .output()
            .expect("ls-files");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[tokio::test]
    async fn stage_is_selective_and_unstage_keeps_worktree_bytes() {
        let (_dir, repo) = temp_repo("selective");
        for name in ["one.txt", "two.txt", "three.txt"] {
            std::fs::write(repo.join(name), "v1\n").expect("write");
        }
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "three"]);
        for name in ["one.txt", "two.txt", "three.txt"] {
            std::fs::write(repo.join(name), "v2\n").expect("write");
        }

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let pick = |name: &str| ids.iter().find(|(n, _)| n == name).unwrap().1.clone();

        // Stage exactly two of three files.
        let snap = core_mutate(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &[pick("one.txt"), pick("two.txt")],
            true,
        )
        .await
        .expect("stage");
        assert!(snap.staged_count == Some(2));
        let entries = staged_entries(&repo);
        assert!(entries.contains("one.txt") && entries.contains("two.txt"));
        // The staged blobs hold v2 while three.txt stays out of the index.
        assert!(!entries.contains("three.txt") || snap.unstaged_count == Some(1));

        // Unstage one file: worktree bytes must be byte-identical.
        let before = std::fs::read(repo.join("one.txt")).expect("read");
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let pick = |name: &str| ids.iter().find(|(n, _)| n == name).unwrap().1.clone();
        let snap = core_mutate(
            &runner,
            &mut registry,
            &repo_id,
            snap.version,
            &[pick("one.txt")],
            false,
        )
        .await
        .expect("unstage");
        let after = std::fs::read(repo.join("one.txt")).expect("read");
        assert_eq!(before, after);
        assert_eq!(snap.staged_count, Some(1));
    }

    #[tokio::test]
    async fn unborn_unstage_keeps_the_file() {
        let (_dir, repo) = temp_repo("unborn");
        std::fs::write(repo.join("first.txt"), "hello\n").expect("write");
        git(&repo, &["add", "first.txt"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let ids = live_ids(&runner, &mut registry, &repo_id).await;

        let snap = core_mutate(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &[ids[0].1.clone()],
            false,
        )
        .await
        .expect("unborn unstage");
        // Index entry gone, file still on disk with its bytes.
        assert_eq!(snap.staged_count, Some(0));
        assert_eq!(
            std::fs::read(repo.join("first.txt")).expect("read"),
            b"hello\n"
        );
    }

    #[tokio::test]
    async fn guards_reject_garbage_before_any_write() {
        let (_dir, repo) = temp_repo("guards");
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let ro_id = open_repo(&mut registry, &repo, TrustState::ReadOnly);
        let ro_version = registry.get(&ro_id).expect("session").version;

        // Read-only sessions cannot mutate.
        let denied = core_mutate(
            &runner,
            &mut registry,
            &ro_id,
            ro_version,
            &["x".into()],
            true,
        )
        .await;
        assert!(matches!(
            denied,
            Err(ref e) if e.code == ErrorCode::TRUST_REQUIRED
        ));

        // Empty selection is a caller bug.
        registry.get_mut(&ro_id).expect("session").trust = TrustState::Trusted;
        let empty = core_mutate(&runner, &mut registry, &ro_id, ro_version, &[], true).await;
        assert!(matches!(
            empty,
            Err(ref e) if e.code == ErrorCode::INVALID_ARGUMENT
        ));

        // Stale versions never write.
        let stale = core_mutate(
            &runner,
            &mut registry,
            &ro_id,
            ro_version.saturating_add(99),
            &["x".into()],
            true,
        )
        .await;
        assert!(matches!(
            stale,
            Err(ref e) if e.code == ErrorCode::STALE_STATE
        ));
        // Nothing was staged behind the rejections.
        assert!(staged_entries(&repo).is_empty());
    }

    #[tokio::test]
    async fn stage_and_discard_hunks_are_exact_and_stale_bound() {
        let (_dir, repo) = temp_repo("hunks");
        let base = (1..=24)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        std::fs::write(repo.join("story.txt"), &base).expect("write base");
        git(&repo, &["add", "story.txt"]);
        git(&repo, &["commit", "-m", "story"]);
        let changed = base
            .replace("line 2\n", "line two changed\n")
            .replace("line 22\n", "line twenty-two changed\n");
        std::fs::write(repo.join("story.txt"), &changed).expect("write changes");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let path_id = ids
            .iter()
            .find(|(path, _)| path == "story.txt")
            .unwrap()
            .1
            .clone();
        let raw_patch = read_worktree_patch(&runner, &repo, b"story.txt")
            .await
            .expect("patch");
        let (hunks, _, _, _) = crate::git::diff::parse_unified_patch(&raw_patch, 5000);
        assert_eq!(hunks.len(), 2, "fixture must produce two independent hunks");

        let staged = core_hunk_mutate(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &path_id,
            &hunks[0].hunk_id,
            None,
        )
        .await
        .expect("stage first hunk");
        let cached = StdCommand::new("git")
            .current_dir(&repo)
            .args(["diff", "--cached"])
            .output()
            .expect("cached diff");
        let cached = String::from_utf8_lossy(&cached.stdout);
        assert!(cached.contains("line two changed"));
        assert!(!cached.contains("line twenty-two changed"));

        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let path_id = ids
            .iter()
            .find(|(path, _)| path == "story.txt")
            .unwrap()
            .1
            .clone();
        let remaining_patch = read_worktree_patch(&runner, &repo, b"story.txt")
            .await
            .expect("remaining patch");
        let (remaining, _, _, _) = crate::git::diff::parse_unified_patch(&remaining_patch, 5000);
        assert_eq!(remaining.len(), 1);
        let confirm = prepare_discard_confirmation(
            &runner,
            &mut registry,
            &repo_id,
            staged.version,
            "discard_hunk",
            &[path_id.clone(), remaining[0].hunk_id.clone()],
        )
        .await
        .expect("prepare hunk discard");
        core_hunk_mutate(
            &runner,
            &mut registry,
            &repo_id,
            staged.version,
            &path_id,
            &remaining[0].hunk_id,
            Some(&confirm.confirmation_token),
        )
        .await
        .expect("discard second hunk");
        let worktree = std::fs::read_to_string(repo.join("story.txt")).expect("read worktree");
        assert!(worktree.contains("line two changed"));
        assert!(worktree.contains("line 22\n"));
        assert!(!worktree.contains("line twenty-two changed"));
    }

    #[tokio::test]
    async fn discard_file_restores_tracked_and_deletes_only_exact_untracked_file() {
        let (_dir, repo) = temp_repo("discard-files");
        std::fs::write(repo.join("tracked.txt"), "base\n").expect("tracked base");
        git(&repo, &["add", "tracked.txt"]);
        git(&repo, &["commit", "-m", "tracked"]);
        std::fs::write(repo.join("tracked.txt"), "changed\n").expect("tracked change");
        std::fs::write(repo.join("untracked*.txt"), "temporary\n").expect("untracked");
        std::fs::write(repo.join("keep.txt"), "keep\n").expect("keep");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let tracked_id = ids
            .iter()
            .find(|(path, _)| path == "tracked.txt")
            .unwrap()
            .1
            .clone();
        let confirm = prepare_discard_confirmation(
            &runner,
            &mut registry,
            &repo_id,
            version,
            "discard_file",
            std::slice::from_ref(&tracked_id),
        )
        .await
        .expect("prepare tracked discard");
        let snap = core_discard_file(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &tracked_id,
            &confirm.confirmation_token,
        )
        .await
        .expect("discard tracked");
        assert_eq!(std::fs::read(repo.join("tracked.txt")).unwrap(), b"base\n");

        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let untracked_id = ids
            .iter()
            .find(|(path, _)| path == "untracked*.txt")
            .unwrap()
            .1
            .clone();
        let confirm = prepare_discard_confirmation(
            &runner,
            &mut registry,
            &repo_id,
            snap.version,
            "discard_file",
            std::slice::from_ref(&untracked_id),
        )
        .await
        .expect("prepare untracked discard");
        std::fs::write(repo.join("untracked*.txt"), "changed after confirm\n")
            .expect("race change");
        let stale = core_discard_file(
            &runner,
            &mut registry,
            &repo_id,
            snap.version,
            &untracked_id,
            &confirm.confirmation_token,
        )
        .await;
        assert!(matches!(stale, Err(ref error) if error.code == ErrorCode::INVALID_ARGUMENT));
        assert!(
            repo.join("untracked*.txt").exists(),
            "stale confirmation must not delete"
        );
        let confirm = prepare_discard_confirmation(
            &runner,
            &mut registry,
            &repo_id,
            snap.version,
            "discard_file",
            std::slice::from_ref(&untracked_id),
        )
        .await
        .expect("prepare fresh untracked discard");
        let final_snapshot = core_discard_file(
            &runner,
            &mut registry,
            &repo_id,
            snap.version,
            &untracked_id,
            &confirm.confirmation_token,
        )
        .await
        .expect("discard untracked");
        assert!(!repo.join("untracked*.txt").exists());
        assert!(
            repo.join("keep.txt").exists(),
            "literal path must not delete a neighbor"
        );

        std::os::unix::fs::symlink("missing-target", repo.join("broken-link"))
            .expect("untracked symlink");
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let link_id = ids
            .iter()
            .find(|(path, _)| path == "broken-link")
            .unwrap()
            .1
            .clone();
        let confirm = prepare_discard_confirmation(
            &runner,
            &mut registry,
            &repo_id,
            final_snapshot.version,
            "discard_file",
            std::slice::from_ref(&link_id),
        )
        .await
        .expect("prepare symlink discard");
        core_discard_file(
            &runner,
            &mut registry,
            &repo_id,
            final_snapshot.version,
            &link_id,
            &confirm.confirmation_token,
        )
        .await
        .expect("discard symlink");
        assert!(std::fs::symlink_metadata(repo.join("broken-link")).is_err());
    }

    #[tokio::test]
    async fn rename_and_magic_names_round_trip() {
        let (_dir, repo) = temp_repo("rename");
        std::fs::write(repo.join("old.txt"), "same\n").expect("write");
        std::fs::write(repo.join("star*.txt"), "magic\n").expect("write");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "base"]);
        // Filesystem move (not `git mv`): the rename starts fully unstaged
        // as a deletion plus an untracked file.
        std::fs::rename(repo.join("old.txt"), repo.join("new.txt")).expect("rename");
        std::fs::write(repo.join("star*.txt"), "magic\nmore\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let version = registry.get(&repo_id).expect("session").version;
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        // The unstaged rename is two rows: deleted old + untracked new.
        let pick = |name: &str| {
            ids.iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .1
                .clone()
        };
        // Staging deletion + new content records R (not A+D); the magic
        // name stages exactly itself, not its glob neighbours.
        let snap = core_mutate(
            &runner,
            &mut registry,
            &repo_id,
            version,
            &[pick("old.txt"), pick("new.txt"), pick("star*.txt")],
            true,
        )
        .await
        .expect("stage");
        assert_eq!(snap.staged_count, Some(2));
        assert_eq!(snap.unstaged_count, Some(0));

        // Unstaging restores the pre-stage state without touching bytes.
        let ids = live_ids(&runner, &mut registry, &repo_id).await;
        let all: Vec<String> = ids.iter().map(|(_, id)| id.clone()).collect();
        let snap = core_mutate(&runner, &mut registry, &repo_id, snap.version, &all, false)
            .await
            .expect("unstage");
        assert_eq!(snap.staged_count, Some(0));
        assert_eq!(
            std::fs::read(repo.join("new.txt")).expect("read"),
            b"same\n"
        );
    }
}
