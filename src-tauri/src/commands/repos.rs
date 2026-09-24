//! Repository lifecycle commands: open / init / snapshot / close / trust / recent.
//!
//! Core logic is plain async fns over runner + registry + store (unit tested
//! against disposable temp repos); thin `#[tauri::command]` wrappers adapt the
//! IPC envelope. Trust defaults to read-only; init implies explicit user
//! intent and therefore starts trusted.

use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::domain::{
    ApiResult, AppError, ErrorCode, HeadState, OpenWorkspaceEntry, RecentEntry, RecoveryAction,
    RepoSnapshot, RepoState, RequestId, SkippedWorkspace, TrustState, WorkspacesRestoreResult,
    WorkspacesSaved,
};
use crate::git::{discover, validate_branch_name, DiscoveredRepo, GitRunner, READ_TIMEOUT};
use crate::persistence::Store;
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

fn store_for(app: &AppHandle) -> Result<Store, AppError> {
    let dir = app.path().app_data_dir().map_err(|_| {
        AppError::new(
            ErrorCode::IO_ERROR,
            "App data directory is unavailable",
            RecoveryAction::RetryRead,
            false,
        )
    })?;
    Ok(Store::open(&dir))
}

async fn read_head(runner: &GitRunner, session: &RepoSession) -> Result<HeadState, AppError> {
    let cwd = &session.worktree_root;
    let symbolic = runner
        .run(cwd, &["symbolic-ref", "-q", "HEAD"], READ_TIMEOUT)
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::GIT_ERROR,
                "Failed to read HEAD",
                RecoveryAction::Refresh,
                true,
            )
        })?;
    if symbolic.success {
        let full = String::from_utf8_lossy(&symbolic.stdout).trim().to_string();
        let name = full
            .strip_prefix("refs/heads/")
            .unwrap_or(&full)
            .to_string();
        let oid_out = runner
            .run(cwd, &["rev-parse", "HEAD"], READ_TIMEOUT)
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::GIT_ERROR,
                    "Failed to read HEAD",
                    RecoveryAction::Refresh,
                    true,
                )
            })?;
        if oid_out.success {
            let oid = String::from_utf8_lossy(&oid_out.stdout).trim().to_string();
            return Ok(HeadState::Branch {
                ref_id: full,
                name,
                oid,
            });
        }
        return Ok(HeadState::Unborn { name });
    }
    let oid_out = runner
        .run(cwd, &["rev-parse", "HEAD"], READ_TIMEOUT)
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::GIT_ERROR,
                "Failed to read HEAD",
                RecoveryAction::Refresh,
                true,
            )
        })?;
    if oid_out.success {
        let oid = String::from_utf8_lossy(&oid_out.stdout).trim().to_string();
        return Ok(HeadState::Detached { oid });
    }
    Err(AppError::new(
        ErrorCode::GIT_ERROR,
        "HEAD is unreadable",
        RecoveryAction::InspectState,
        false,
    ))
}

pub(crate) async fn build_snapshot(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    session: &RepoSession,
) -> Result<RepoSnapshot, AppError> {
    let head = read_head(runner, session).await?;
    // Trusted sessions read live counts (and refresh the cached listing so
    // path ids stay current); read-only stays null = "unavailable until
    // trusted", never a fake zero. A failed count read degrades to null.
    let trusted = session.trust == TrustState::Trusted;
    let (staged_count, unstaged_count, conflict_count) = super::status::snapshot_counts(
        runner,
        registry,
        &session.repo_id,
        &session.worktree_root,
        trusted,
    )
    .await;
    // Sync state is local-only (no network): effective upstream plus the
    // last fetch time. Degrades to None, never fails the snapshot.
    let (upstream, _) = super::sync::upstream_summary(runner, &session.worktree_root).await;
    let last_fetch_at = registry
        .fetch_time(&session.repo_id)
        .map(super::sync::epoch_to_iso8601);
    // Heal overrides whose repo marker is gone: a merge finished outside
    // the app (terminal commit/abort) clears Merging; zero unmerged files
    // clears StashConflict. Explicit commands still set the state first.
    let merge_head_live = session.git_dir.join("MERGE_HEAD").exists();
    if !merge_head_live && registry.state_of(&session.repo_id) == RepoState::Merging {
        registry.set_repo_state(&session.repo_id, RepoState::Normal);
    }
    if conflict_count == Some(0) && registry.state_of(&session.repo_id) == RepoState::StashConflict
    {
        registry.set_repo_state(&session.repo_id, RepoState::Normal);
    }
    // App-origin merges carry a registry record; anything else with a live
    // MERGE_HEAD (terminal, another tool) is external.
    let merge_origin = if registry.merge_get(&session.repo_id).is_some() {
        Some("app".to_string())
    } else if merge_head_live {
        Some("external".to_string())
    } else {
        None
    };
    Ok(RepoSnapshot {
        repo_id: session.repo_id.clone(),
        workspace_key: session.workspace_key(),
        version: session.version,
        display_name: session.display_name.clone(),
        display_path: session.display_path.clone(),
        head,
        trust: session.trust.clone(),
        state: registry.state_of(&session.repo_id),
        merge_origin,
        upstream,
        last_fetch_at,
        active_operation: None,
        staged_count,
        unstaged_count,
        conflict_count,
    })
}

/// Best-effort invalidation broadcast. Listeners must still query
/// `operation_get`/snapshot after (re)connect; a missed event is never the
/// only signal. Never carries diffs or secrets — only ids and a version.
fn emit_invalidated(app: &AppHandle, repo_id: &str, version: u64, reason: &str) {
    #[derive(Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Invalidated<'a> {
        repo_id: &'a str,
        version: u64,
        reason: &'a str,
    }
    let _ = app.emit(
        "gitdock://repo-invalidated",
        Invalidated {
            repo_id,
            version,
            reason,
        },
    );
}

async fn core_open(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    store: &mut Store,
    selected: &Path,
) -> Result<RepoSnapshot, AppError> {
    let discovered = discover(runner, selected).await?;
    if discovered.bare {
        return Err(AppError::new(
            ErrorCode::BARE_REPOSITORY,
            "Selected folder is a bare repository",
            RecoveryAction::ChooseRepository,
            false,
        ));
    }
    open_discovered(runner, registry, store, &discovered).await
}

async fn open_discovered(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    store: &mut Store,
    discovered: &DiscoveredRepo,
) -> Result<RepoSnapshot, AppError> {
    let key = discovered.common_dir.to_string_lossy().into_owned();
    let trust = if store.is_trusted(&key) {
        TrustState::Trusted
    } else {
        TrustState::ReadOnly
    };
    let session = registry.open(discovered, trust);
    store.push_recent(&session.workspace_key(), &session.display_path);
    build_snapshot(runner, registry, &session).await
}

async fn core_init(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    store: &mut Store,
    selected: &Path,
    branch: &str,
) -> Result<RepoSnapshot, AppError> {
    if selected.exists() && discover(runner, selected).await.is_ok() {
        return Err(bad_request(
            "Selected folder is already a Git repository; init refuses to overwrite it",
        ));
    }
    if !selected.exists() {
        std::fs::create_dir_all(selected).map_err(|_| {
            AppError::new(
                ErrorCode::PATH_INVALID,
                "Destination folder cannot be created",
                RecoveryAction::ChooseRepository,
                false,
            )
        })?;
    }
    validate_branch_name(runner, &PathBuf::from("/"), branch).await?;
    let parent = selected.parent().unwrap_or_else(|| Path::new("/"));
    let target = selected
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| bad_request("Destination folder name is invalid"))?;
    let init_out = runner
        .run(parent, &["init", "-b", branch, &target], READ_TIMEOUT)
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::GIT_ERROR,
                "Git init failed",
                RecoveryAction::ChooseRepository,
                false,
            )
        })?;
    if !init_out.success {
        return Err(AppError::new(
            ErrorCode::GIT_ERROR,
            "Git init failed",
            RecoveryAction::ChooseRepository,
            false,
        ));
    }
    let discovered = discover(runner, selected).await?;
    // Init is explicit user intent, so it starts trusted (unlike open).
    let session = registry.open(&discovered, TrustState::Trusted);
    let key = session.key();
    store.set_trusted(&key, true);
    store.push_recent(&session.workspace_key(), &session.display_path);
    build_snapshot(runner, registry, &session).await
}

async fn core_snapshot(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    refresh: bool,
) -> Result<RepoSnapshot, AppError> {
    if refresh {
        registry.bump(repo_id).ok_or_else(session_missing)?;
    }
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    build_snapshot(runner, registry, &session).await
}

async fn core_trust_set(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    store: &mut Store,
    repo_id: &str,
    trusted: bool,
) -> Result<RepoSnapshot, AppError> {
    {
        let session = registry.get_mut(repo_id).ok_or_else(session_missing)?;
        session.trust = if trusted {
            TrustState::Trusted
        } else {
            TrustState::ReadOnly
        };
        session.version = session.version.saturating_add(1);
        store.set_trusted(&session.key(), trusted);
    }
    core_snapshot(runner, registry, repo_id, false).await
}

async fn core_close(registry: &mut RepoRegistry, repo_id: &str) -> Result<bool, AppError> {
    if registry.get(repo_id).is_none() {
        return Err(session_missing());
    }
    // Never orphan an unsettled mutation: close waits for terminal state.
    if registry.has_active_operation(repo_id) {
        return Err(AppError::new(
            ErrorCode::REPO_BUSY,
            "An operation is still running; wait for it or cancel it before closing",
            RecoveryAction::InspectState,
            false,
        ));
    }
    registry.evict_repo(repo_id);
    Ok(registry.close(repo_id))
}

// ---- IPC request/response types (camelCase) ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoOpenRequest {
    pub request_id: RequestId,
    pub selected_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInitRequest {
    pub request_id: RequestId,
    pub selected_path: String,
    pub initial_branch: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoIdRequest {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSnapshotRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub refresh: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoTrustRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub trusted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentListRequest {
    pub request_id: RequestId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRemoveRequest {
    pub request_id: RequestId,
    pub entry_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedResponse {
    pub closed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedResponse {
    pub removed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesSaveRequest {
    pub request_id: RequestId,
    pub entries: Vec<OpenWorkspaceEntry>,
    pub active_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesRestoreRequest {
    pub request_id: RequestId,
}

// ---- Commands ----

#[tauri::command]
pub async fn repo_open(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoOpenRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let mut store = store_for(&app)?;
        core_open(
            &runner,
            &mut registry,
            &mut store,
            Path::new(&request.selected_path),
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
pub async fn repo_snapshot(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoSnapshotRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let refresh = request.refresh;
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let _ = store_for(&app)?;
        core_snapshot(&runner, &mut registry, &request.repo_id, refresh).await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => {
            if refresh {
                emit_invalidated(&app, &snapshot.repo_id, snapshot.version, "refresh");
            }
            ApiResult::ok(snapshot, request_id)
        }
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn repo_close(
    _app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoIdRequest,
) -> Result<ApiResult<ClosedResponse>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let mut registry = registry.lock().await;
        core_close(&mut registry, &request.repo_id)
            .await
            .map(|_| ClosedResponse { closed: true })
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn repo_init(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoInitRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let mut store = store_for(&app)?;
        core_init(
            &runner,
            &mut registry,
            &mut store,
            Path::new(&request.selected_path),
            &request.initial_branch,
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
pub async fn repo_trust_set(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoTrustRequest,
) -> Result<ApiResult<RepoSnapshot>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let mut store = store_for(&app)?;
        core_trust_set(
            &runner,
            &mut registry,
            &mut store,
            &request.repo_id,
            request.trusted,
        )
        .await
    }
    .await;
    Ok(match result {
        Ok(snapshot) => {
            emit_invalidated(&app, &snapshot.repo_id, snapshot.version, "trust");
            ApiResult::ok(snapshot, request_id)
        }
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn repo_recent_list(
    app: AppHandle,
    request: RecentListRequest,
) -> Result<ApiResult<Vec<RecentEntry>>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let store = store_for(&app)?;
        Ok::<_, AppError>(store.recents().to_vec())
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn repo_recent_remove(
    app: AppHandle,
    request: RecentRemoveRequest,
) -> Result<ApiResult<RemovedResponse>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let mut store = store_for(&app)?;
        if store.remove_recent(&request.entry_id) {
            Ok(RemovedResponse { removed: true })
        } else {
            Err(bad_request("Unknown recent entry"))
        }
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn workspaces_save(
    app: AppHandle,
    request: WorkspacesSaveRequest,
) -> Result<ApiResult<WorkspacesSaved>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        if request.entries.len() > 100 {
            return Err(bad_request("Too many workspaces to save"));
        }
        let mut store = store_for(&app)?;
        Ok::<_, AppError>(core_workspaces_save(
            &mut store,
            request.entries,
            request.active_key,
        ))
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn workspaces_restore(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: WorkspacesRestoreRequest,
) -> Result<ApiResult<WorkspacesRestoreResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let mut store = store_for(&app)?;
        core_workspaces_restore(&runner, &mut registry, &mut store).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

/// Decode a `worktree-v1:<hex>` workspace key back to its worktree root.
/// Only keys the backend itself issued are accepted; anything else fails
/// closed so a tampered settings file cannot point Git at odd paths.
/// Reopening still goes through full discovery + validation.
fn decode_workspace_key(key: &str) -> Result<PathBuf, AppError> {
    let hex = key
        .strip_prefix("worktree-v1:")
        .ok_or_else(|| bad_request("Unknown saved workspace"))?;
    if hex.is_empty() || hex.len() > 4096 || hex.len() % 2 != 0 {
        return Err(bad_request("Unknown saved workspace"));
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let raw = hex.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        let pair = std::str::from_utf8(&raw[index..index + 2])
            .ok()
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .ok_or_else(|| bad_request("Unknown saved workspace"))?;
        bytes.push(pair);
        index += 2;
    }
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(OsStr::from_bytes(&bytes)))
    }
    #[cfg(not(unix))]
    {
        Ok(PathBuf::from(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

fn core_workspaces_save(
    store: &mut Store,
    entries: Vec<OpenWorkspaceEntry>,
    active_key: Option<String>,
) -> WorkspacesSaved {
    store.save_open_workspaces(entries, active_key);
    WorkspacesSaved { saved: true }
}

async fn core_workspaces_restore(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    store: &mut Store,
) -> Result<WorkspacesRestoreResult, AppError> {
    let entries = store.open_workspaces().to_vec();
    let active_key = store.active_workspace().map(str::to_string);
    let mut opened = Vec::new();
    let mut skipped = Vec::new();
    for entry in &entries {
        let path = match decode_workspace_key(&entry.key) {
            Ok(path) => path,
            Err(error) => {
                skipped.push(SkippedWorkspace {
                    key: entry.key.clone(),
                    display_path: entry.display_path.clone(),
                    code: error.code,
                    message: error.message,
                });
                continue;
            }
        };
        match core_open(runner, registry, store, &path).await {
            Ok(snapshot) => opened.push(snapshot),
            Err(error) => skipped.push(SkippedWorkspace {
                key: entry.key.clone(),
                display_path: entry.display_path.clone(),
                code: error.code,
                message: error.message,
            }),
        }
    }
    // Repos that vanished are pruned; still-listed recents remain the
    // manual fallback, so the next launch does not retry dead paths.
    let drop_keys: Vec<String> = skipped.iter().map(|skip| skip.key.clone()).collect();
    store.prune_open_workspaces(&drop_keys);
    Ok(WorkspacesRestoreResult {
        opened,
        skipped,
        active_key,
    })
}

pub mod prelude {
    pub use super::{
        repo_close, repo_init, repo_open, repo_recent_list, repo_recent_remove, repo_snapshot,
        repo_trust_set, workspaces_restore, workspaces_save,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("gitdock-t03-{}-{}-{label}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn git(cwd: &Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed in {}", cwd.display());
    }

    fn commit_file(repo: &Path, name: &str) {
        std::fs::write(repo.join(name), "content\n").expect("write file");
        git(repo, &["add", name]);
        git(
            repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-m",
                "test commit",
            ],
        );
    }

    fn harness() -> (GitRunner, RepoRegistry, Store, PathBuf) {
        let root = temp_root("harness");
        let store_dir = root.join("store");
        std::fs::create_dir_all(&store_dir).expect("store dir");
        let exe = GitRunner::resolve_from_path().expect("system git");
        (
            GitRunner::new(exe),
            RepoRegistry::default(),
            Store::open(&store_dir),
            root,
        )
    }

    #[tokio::test]
    async fn open_ordinary_and_subfolder_share_root() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("sub")).expect("mkdir");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt");

        let from_root = core_open(&runner, &mut registry, &mut store, &repo)
            .await
            .expect("open root");
        let from_sub = core_open(&runner, &mut registry, &mut store, &repo.join("sub"))
            .await
            .expect("open subfolder");
        assert_eq!(from_root.repo_id, from_sub.repo_id, "same session reused");
        assert_eq!(from_root.workspace_key, from_sub.workspace_key);
        match from_root.head {
            HeadState::Branch { name, oid, .. } => {
                assert_eq!(name, "main");
                assert!(!oid.is_empty());
            }
            other => panic!("expected branch head, got {other:?}"),
        }
        assert!(matches!(from_root.trust, TrustState::ReadOnly));
    }

    #[tokio::test]
    async fn linked_worktree_discovers() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt");
        git(&repo, &["worktree", "add", "../linked"]);
        let linked = root.join("linked");
        assert!(linked.join(".git").is_file(), ".git must be a file");

        let snapshot = core_open(&runner, &mut registry, &mut store, &linked)
            .await
            .expect("open linked worktree");
        assert!(snapshot.display_path.ends_with("linked"));
    }

    #[tokio::test]
    async fn multiple_repositories_close_independently_and_keep_stable_draft_keys() {
        let (runner, mut registry, mut store, root) = harness();
        git(&root, &["init", "-b", "main", "first"]);
        git(&root, &["init", "-b", "develop", "second"]);
        let first = core_open(&runner, &mut registry, &mut store, &root.join("first"))
            .await
            .expect("open first");
        let second = core_open(&runner, &mut registry, &mut store, &root.join("second"))
            .await
            .expect("open second");
        assert_ne!(first.repo_id, second.repo_id);
        assert_ne!(first.workspace_key, second.workspace_key);
        core_close(&mut registry, &first.repo_id)
            .await
            .expect("close first only");
        let still_open = core_snapshot(&runner, &mut registry, &second.repo_id, false)
            .await
            .expect("second remains open");
        assert_eq!(still_open.workspace_key, second.workspace_key);
        let reopened = core_open(&runner, &mut registry, &mut store, &root.join("first"))
            .await
            .expect("reopen first");
        assert_ne!(reopened.repo_id, first.repo_id);
        assert_eq!(
            reopened.workspace_key, first.workspace_key,
            "draft key survives close/reopen"
        );
    }

    #[tokio::test]
    async fn linked_worktree_tabs_have_distinct_heads_but_share_write_queue() {
        let (runner, mut registry, mut store, root) = harness();
        git(&root, &["init", "-b", "main", "repo"]);
        let repo = root.join("repo");
        commit_file(&repo, "a.txt");
        git(&repo, &["worktree", "add", "../linked"]);
        let first = core_open(&runner, &mut registry, &mut store, &repo)
            .await
            .expect("open main worktree");
        let linked = core_open(&runner, &mut registry, &mut store, &root.join("linked"))
            .await
            .expect("open linked worktree alongside main");
        assert_ne!(first.repo_id, linked.repo_id);
        assert_ne!(first.workspace_key, linked.workspace_key);
        assert_eq!(
            store.recents().len(),
            2,
            "both worktrees remain in recent repositories"
        );
        assert!(matches!(first.head, HeadState::Branch { ref name, .. } if name == "main"));
        assert!(matches!(linked.head, HeadState::Branch { ref name, .. } if name == "linked"));
        let first_key = registry.get(&first.repo_id).unwrap().key();
        let linked_key = registry.get(&linked.repo_id).unwrap().key();
        assert!(std::sync::Arc::ptr_eq(
            &registry.queue_for(&first_key),
            &registry.queue_for(&linked_key)
        ));
        core_close(&mut registry, &linked.repo_id)
            .await
            .expect("close linked only");
        assert!(registry.get(&first.repo_id).is_some());
    }

    #[tokio::test]
    async fn closing_a_busy_tab_preserves_its_session_and_other_tabs() {
        use crate::domain::{OperationRecord, OperationState};
        let (runner, mut registry, mut store, root) = harness();
        git(&root, &["init", "-b", "main", "busy"]);
        git(&root, &["init", "-b", "main", "idle"]);
        let busy = core_open(&runner, &mut registry, &mut store, &root.join("busy"))
            .await
            .expect("open busy");
        let idle = core_open(&runner, &mut registry, &mut store, &root.join("idle"))
            .await
            .expect("open idle");
        registry
            .job_register(
                OperationRecord {
                    operation_id: "tab-job".into(),
                    request_id: "tab-request".into(),
                    repo_id: Some(busy.repo_id.clone()),
                    kind: "remote.fetch".into(),
                    state: OperationState::Running,
                    stage: "fetching".into(),
                    progress: None,
                    error_code: None,
                    error: None,
                },
                "tab-fetch",
            )
            .expect("register job");
        let error = core_close(&mut registry, &busy.repo_id)
            .await
            .expect_err("busy close must fail");
        assert_eq!(error.code, ErrorCode::REPO_BUSY);
        assert!(registry.get(&busy.repo_id).is_some());
        core_close(&mut registry, &idle.repo_id)
            .await
            .expect("idle tab can close independently");
        assert!(registry.get(&busy.repo_id).is_some());
        registry.job_set_state("tab-job", OperationState::Succeeded);
        core_close(&mut registry, &busy.repo_id)
            .await
            .expect("settled tab can close");
    }

    #[tokio::test]
    async fn unborn_head_opens() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "trunk", "repo"]);

        let snapshot = core_open(&runner, &mut registry, &mut store, &repo)
            .await
            .expect("open unborn");
        match snapshot.head {
            HeadState::Unborn { name } => assert_eq!(name, "trunk"),
            other => panic!("expected unborn head, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bare_missing_and_file_rejected() {
        let (runner, mut registry, mut store, root) = harness();
        let bare = root.join("bare.git");
        git(&root, &["init", "--bare", "bare.git"]);
        let err = core_open(&runner, &mut registry, &mut store, &bare)
            .await
            .expect_err("bare must be rejected");
        assert_eq!(err.code, ErrorCode::BARE_REPOSITORY);

        let err = core_open(&runner, &mut registry, &mut store, &root.join("nope"))
            .await
            .expect_err("missing must be rejected");
        assert_eq!(err.code, ErrorCode::REPO_UNAVAILABLE);

        let plain = root.join("plain");
        std::fs::create_dir_all(&plain).expect("mkdir");
        let err = core_open(&runner, &mut registry, &mut store, &plain)
            .await
            .expect_err("non-repo must be rejected");
        assert_eq!(err.code, ErrorCode::NOT_REPOSITORY);

        let file = root.join("f.txt");
        std::fs::write(&file, "x").expect("write");
        let err = core_open(&runner, &mut registry, &mut store, &file)
            .await
            .expect_err("file must be rejected");
        assert_eq!(err.code, ErrorCode::PATH_INVALID);
    }

    #[tokio::test]
    async fn init_flow_and_refusals() {
        let (runner, mut registry, mut store, root) = harness();
        let target = root.join("fresh");
        let snapshot = core_init(&runner, &mut registry, &mut store, &target, "main")
            .await
            .expect("init");
        assert!(matches!(snapshot.trust, TrustState::Trusted));
        assert!(matches!(snapshot.head, HeadState::Unborn { .. }));
        assert!(target.join(".git").is_dir());

        let err = core_init(&runner, &mut registry, &mut store, &target, "main")
            .await
            .expect_err("init must refuse existing repo");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        let err = core_init(&runner, &mut registry, &mut store, &root.join("x"), "-bad")
            .await
            .expect_err("option-like branch refused");
        assert!(matches!(
            err.code,
            ErrorCode::INVALID_ARGUMENT | ErrorCode::REF_INVALID
        ));
    }

    #[tokio::test]
    async fn detached_head_and_trust_roundtrip() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt");
        git(&repo, &["checkout", "--detach", "HEAD"]);

        let snapshot = core_open(&runner, &mut registry, &mut store, &repo)
            .await
            .expect("open detached");
        assert!(matches!(snapshot.head, HeadState::Detached { .. }));
        let repo_id = snapshot.repo_id.clone();

        core_trust_set(&runner, &mut registry, &mut store, &repo_id, true)
            .await
            .expect("trust set");
        let session = registry.get(&repo_id).expect("session");
        assert!(matches!(session.trust, TrustState::Trusted));
        assert!(store.is_trusted(&session.key()));

        let version_before = session.version;
        let refreshed = core_snapshot(&runner, &mut registry, &repo_id, true)
            .await
            .expect("refresh");
        assert_eq!(refreshed.version, version_before + 1);

        assert!(core_close(&mut registry, &repo_id).await.is_ok());
        assert!(core_close(&mut registry, &repo_id).await.is_err());
        assert!(core_snapshot(&runner, &mut registry, "nope", false)
            .await
            .is_err());
    }

    #[test]
    fn recent_list_caps_at_twenty() {
        let (_runner, _registry, mut store, _root) = harness();
        for i in 0..25 {
            store.push_recent(&format!("/tmp/repo-{i}"), &format!("repo-{i}"));
        }
        assert_eq!(store.recents().len(), 20);
        assert_eq!(store.recents()[0].key, "/tmp/repo-24");
        assert!(store.remove_recent("/tmp/repo-24"));
        assert_eq!(store.recents().len(), 19);
        assert!(!store.remove_recent("/tmp/repo-24"));
    }

    // Tests exercise the shared core_* fns above through the same paths as commands.

    fn make_repo(root: &Path, name: &str) -> PathBuf {
        let repo = root.join(name);
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(root, &["init", "-b", "main", name]);
        commit_file(&repo, "a.txt");
        repo
    }

    #[tokio::test]
    async fn workspaces_save_restore_round_trip() {
        let (runner, mut registry, mut store, root) = harness();
        let first = make_repo(&root, "one");
        let second = make_repo(&root, "two");
        let snap_one = core_open(&runner, &mut registry, &mut store, &first)
            .await
            .expect("open one");
        let snap_two = core_open(&runner, &mut registry, &mut store, &second)
            .await
            .expect("open two");
        let entries = vec![
            OpenWorkspaceEntry {
                key: snap_one.workspace_key.clone(),
                display_path: snap_one.display_path.clone(),
            },
            OpenWorkspaceEntry {
                key: snap_two.workspace_key.clone(),
                display_path: snap_two.display_path.clone(),
            },
        ];
        let saved = core_workspaces_save(&mut store, entries, Some(snap_two.workspace_key.clone()));
        assert!(saved.saved);

        // A fresh registry simulates an app restart: both repos reopen and
        // the active key survives.
        let mut fresh = RepoRegistry::default();
        let result = core_workspaces_restore(&runner, &mut fresh, &mut store)
            .await
            .expect("restore");
        assert_eq!(result.opened.len(), 2);
        assert!(result.skipped.is_empty());
        assert_eq!(
            result.active_key.as_deref(),
            Some(snap_two.workspace_key.as_str())
        );
        assert_eq!(store.open_workspaces().len(), 2);
    }

    #[tokio::test]
    async fn workspaces_restore_skips_missing_and_prunes() {
        let (runner, mut registry, mut store, root) = harness();
        let kept = make_repo(&root, "kept");
        let gone = make_repo(&root, "gone");
        let snap_kept = core_open(&runner, &mut registry, &mut store, &kept)
            .await
            .expect("open kept");
        let snap_gone = core_open(&runner, &mut registry, &mut store, &gone)
            .await
            .expect("open gone");
        core_workspaces_save(
            &mut store,
            vec![
                OpenWorkspaceEntry {
                    key: snap_kept.workspace_key.clone(),
                    display_path: snap_kept.display_path.clone(),
                },
                OpenWorkspaceEntry {
                    key: snap_gone.workspace_key.clone(),
                    display_path: snap_gone.display_path.clone(),
                },
            ],
            Some(snap_gone.workspace_key.clone()),
        );
        std::fs::remove_dir_all(&gone).expect("delete repo");

        let mut fresh = RepoRegistry::default();
        let result = core_workspaces_restore(&runner, &mut fresh, &mut store)
            .await
            .expect("restore");
        assert_eq!(result.opened.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].key, snap_gone.workspace_key);
        // Dead entries are pruned so the next launch does not retry them;
        // the dropped active key falls back to null.
        assert_eq!(store.open_workspaces().len(), 1);
        assert_eq!(store.active_workspace(), None);
    }

    #[test]
    fn workspaces_save_drops_forged_keys() {
        let (_runner, _registry, mut store, _root) = harness();
        core_workspaces_save(
            &mut store,
            vec![
                OpenWorkspaceEntry {
                    key: "worktree-v1:2f746d70".to_string(),
                    display_path: "/tmp".to_string(),
                },
                OpenWorkspaceEntry {
                    key: "/etc/passwd".to_string(),
                    display_path: "/etc/passwd".to_string(),
                },
                OpenWorkspaceEntry {
                    key: "worktree-v1:2f746d70".to_string(),
                    display_path: "/tmp".to_string(),
                },
            ],
            Some("/etc/passwd".to_string()),
        );
        assert_eq!(store.open_workspaces().len(), 1);
        assert_eq!(store.active_workspace(), None);
        assert!(decode_workspace_key("worktree-v1:2f746d70").is_ok());
        assert!(decode_workspace_key("/etc/passwd").is_err());
        assert!(decode_workspace_key("worktree-v1:zz").is_err());
    }
}
