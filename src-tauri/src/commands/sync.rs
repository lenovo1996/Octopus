//! Read-only sync state plus background fetch/pull/push jobs (T11).
//!
//! `remote_status` never touches the network: it reports the effective
//! upstream, local ahead/behind counts and the last fetch time. Mutations
//! run as cancellable background jobs with progress events, an
//! `operation_get` fallback and an append-only redacted operation log.
//! Auth uses the existing credential helper / SSH agent in trusted
//! sessions only; prompts are disabled so failures classify fast.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, BitbucketConnectionResult, ErrorCode, OperationLogPage, OperationRecord,
    OperationStarted, OperationState, RecoveryAction, RemoteStatus, RequestId, TrustState,
};
use crate::git::{
    ahead_behind, classify_network_stderr, is_push_rejected, parse_progress_line, redact_url,
    resolve_upstream, validate_remote_url, GitRunner, NetworkFault, NETWORK_TIMEOUT,
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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// ISO-8601 UTC (`2026-09-22T10:00:00Z`) without extra dependencies.
pub fn epoch_to_iso8601(secs: u64) -> String {
    let days = secs / 86_400;
    let time = secs % 86_400;
    let (hour, minute, second) = (time / 3600, (time % 3600) / 60, time % 60);
    // Howard Hinnant's civil-from-days algorithm.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u64;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u64;
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn emit_operation(app: &AppHandle, record: &OperationRecord) {
    let _ = app.emit("gitdock://operation", record);
}

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

/// Shared read for `remote_status` and snapshot filling: effective upstream
/// plus local ahead/behind. Degrades to `None` fields, never an error, when
/// no upstream is configured.
pub(crate) async fn upstream_summary(
    runner: &GitRunner,
    worktree_root: &std::path::Path,
) -> (Option<crate::domain::UpstreamInfo>, Option<String>) {
    let upstream = match resolve_upstream(runner, worktree_root).await {
        Ok(Some(up)) => up,
        _ => return (None, None),
    };
    let (ahead, behind) = match ahead_behind(runner, worktree_root, &upstream.full_name).await {
        Ok(Some((a, b))) => (a, b),
        _ => (0, 0),
    };
    (
        Some(crate::domain::UpstreamInfo {
            ref_id: upstream.full_name,
            ahead,
            behind,
        }),
        Some(upstream.url),
    )
}

async fn remote_name_for(runner: &GitRunner, cwd: &std::path::Path) -> Option<String> {
    let head = runner
        .run(
            cwd,
            &["symbolic-ref", "-q", "--short", "HEAD"],
            crate::git::READ_TIMEOUT,
        )
        .await
        .ok()?;
    if !head.success {
        return None;
    }
    let branch = String::from_utf8_lossy(&head.stdout).trim().to_string();
    runner
        .run(
            cwd,
            &["config", "--get", &format!("branch.{branch}.remote")],
            crate::git::READ_TIMEOUT,
        )
        .await
        .ok()
        .filter(|out| out.success)
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|name| !name.is_empty())
}

// ---- job plumbing ----

fn new_operation(repo_id: &str, request_id: &str, kind: &str) -> OperationRecord {
    OperationRecord {
        operation_id: uuid::Uuid::new_v4().to_string(),
        request_id: request_id.to_string(),
        repo_id: Some(repo_id.to_string()),
        kind: kind.to_string(),
        state: OperationState::Queued,
        stage: "queued".to_string(),
        progress: None,
        error_code: None,
        error: None,
    }
}

pub(crate) fn network_error(stderr: &str) -> AppError {
    if is_push_rejected(stderr) {
        return AppError::new(
            ErrorCode::DIVERGED,
            "Push was rejected because the remote tip differs from yours. Fetch and merge, or force-push if you rewrote local history",
            RecoveryAction::Refresh,
            true,
        );
    }
    match classify_network_stderr(stderr) {
        NetworkFault::Offline => AppError::new(
            ErrorCode::OFFLINE,
            "Network is unreachable; check the connection and retry",
            RecoveryAction::RetryRead,
            true,
        ),
        NetworkFault::Auth => AppError::new(
            ErrorCode::AUTH_REQUIRED,
            "Authentication was rejected. Refresh your HTTPS credentials or unlock your SSH key outside Octopus, then retry",
            RecoveryAction::Authenticate,
            false,
        ),
        NetworkFault::Other => {
            AppError::new(
                ErrorCode::GIT_ERROR,
                "The remote operation failed. Verify the remote and branch state, then retry",
                RecoveryAction::Refresh,
                true,
            )
        }
    }
}

/// Outcome of one spawned remote command, independent of app state so tests
/// can drive it directly (including a slow stand-in program for cancel).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum JobOutcome {
    Ok,
    Cancelled,
    Failed(AppError),
}

/// Spawn `program argv` with piped stderr, forward `--progress` percentages
/// through `progress`, and kill the child when `cancel` turns true.
/// Bounded by `NETWORK_TIMEOUT`.
pub(crate) async fn execute_remote(
    program: &str,
    argv: &[String],
    worktree_root: &std::path::Path,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    progress: tokio::sync::mpsc::UnboundedSender<(u32, String)>,
) -> JobOutcome {
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut command = tokio::process::Command::new(program);
    command
        .current_dir(worktree_root)
        .args(argv)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for key in crate::git::runner::SANITIZED_GIT_ENV {
        command.env_remove(key);
    }
    let mut child = match command.spawn() {
        Err(_) => {
            return JobOutcome::Failed(AppError::new(
                ErrorCode::GIT_ERROR,
                "Git could not start the remote operation",
                RecoveryAction::ConfigureGit,
                false,
            ))
        }
        Ok(child) => child,
    };
    let stderr = child.stderr.take();
    let mut lines = stderr.map(BufReader::new).map(|r| r.lines());
    let mut buffered_err = String::new();
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return JobOutcome::Cancelled;
        }
        let next_line = async {
            match &mut lines {
                Some(reader) => reader.next_line().await.ok().flatten(),
                None => None,
            }
        };
        match tokio::time::timeout(std::time::Duration::from_millis(200), next_line).await {
            Ok(Some(line)) => {
                if buffered_err.len() < 4000 {
                    buffered_err.push_str(&line);
                    buffered_err.push('\n');
                }
                if let Some(pct) = parse_progress_line(&line) {
                    let stage = line.split(':').next().unwrap_or("").trim().to_lowercase();
                    let _ = progress.send((pct, stage));
                }
            }
            Ok(None) => match tokio::time::timeout(NETWORK_TIMEOUT, child.wait()).await {
                Ok(Ok(status)) if status.success() => return JobOutcome::Ok,
                Ok(Ok(_)) => {
                    let error = network_error(&buffered_err);
                    return JobOutcome::Failed(error);
                }
                _ => {
                    let _ = child.kill().await;
                    return JobOutcome::Failed(AppError::new(
                        ErrorCode::TIMEOUT,
                        "The remote operation timed out; check the connection and retry",
                        RecoveryAction::RetryRead,
                        true,
                    ));
                }
            },
            Err(_) => continue,
        }
    }
}

/// Background body shared by fetch/pull/push: mark Running, run the child
/// through [`execute_remote`] while relaying progress and polling the cancel
/// flag, then record the terminal state, the redacted log row and the
/// invalidation broadcast.
async fn run_remote_job(
    app: AppHandle,
    kind: &'static str,
    repo_id: String,
    operation_id: String,
    worktree_root: std::path::PathBuf,
    argv: Vec<String>,
    log_summary: String,
) {
    use std::sync::atomic::{AtomicBool, Ordering};

    let record_state = |registry: &RepoRegistry| registry.job_get(&operation_id).cloned();
    let store: State<'_, Mutex<RepoRegistry>> = app.state();
    {
        let mut registry = store.lock().await;
        registry.job_set_state(&operation_id, OperationState::Running);
        registry.job_set_progress(&operation_id, "contacting remote", None);
        if let Some(record) = record_state(&registry) {
            emit_operation(&app, &record);
        }
    }

    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut exec = Box::pin(execute_remote(
        "git",
        &argv,
        &worktree_root,
        cancel.clone(),
        progress_tx,
    ));
    let outcome: JobOutcome = loop {
        tokio::select! {
            biased;
            Some((pct, stage)) = progress_rx.recv() => {
                let mut registry = store.lock().await;
                registry.job_set_progress(&operation_id, &stage, Some(pct));
                if let Some(record) = record_state(&registry) {
                    emit_operation(&app, &record);
                }
            }
            done = &mut exec => break done,
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if store.lock().await.job_cancel_requested(&operation_id) {
                    cancel.store(true, Ordering::SeqCst);
                }
            }
        }
    };
    let (state, error) = match outcome {
        JobOutcome::Ok => (OperationState::Succeeded, None),
        JobOutcome::Cancelled => (OperationState::Cancelled, None),
        JobOutcome::Failed(error) => (OperationState::Failed, Some(error)),
    };
    let error_code = error.as_ref().map(|value| format!("{:?}", value.code));
    {
        let mut registry = store.lock().await;
        registry.job_finish(&operation_id, state.clone(), error);
        let outcome_str = match state {
            OperationState::Succeeded => "ok",
            OperationState::Cancelled => "cancelled",
            _ => "error",
        };
        registry.log_push(
            &repo_id,
            kind,
            &log_summary,
            outcome_str,
            error_code.clone(),
            now_ms(),
        );
        if matches!(state, OperationState::Succeeded) {
            if kind == "fetch" {
                registry.fetch_recorded(&repo_id, now_ms() / 1000);
            }
            registry.invalidate_history(&repo_id);
            registry.bump(&repo_id);
            let version = registry.get(&repo_id).map(|s| s.version).unwrap_or(0);
            emit_invalidated(&app, &repo_id, version, kind);
        }
        if let Some(record) = record_state(&mut registry) {
            emit_operation(&app, &record);
        }
    }
}

/// Read and policy-check a configured remote URL. This raw form must remain
/// inside the backend and must never be logged or returned over IPC.
async fn checked_raw_remote_url(
    runner: &GitRunner,
    cwd: &std::path::Path,
    name: &str,
) -> Result<String, AppError> {
    let out = runner
        .run(
            cwd,
            &["config", "--get", &format!("remote.{name}.url")],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("Cannot read the remote URL"))?;
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.success || raw.is_empty() {
        return Err(bad_request("Remote has no URL configured"));
    }
    validate_remote_url(&raw).map_err(|e| match e {
        crate::git::UrlError::Rejected(message) => AppError::new(
            ErrorCode::UNSUPPORTED,
            message,
            RecoveryAction::InspectState,
            false,
        ),
    })?;
    Ok(raw)
}

/// Read and policy-check a configured remote URL. The returned string is
/// already redacted and safe for logs and UI summaries.
async fn checked_remote_url(
    runner: &GitRunner,
    cwd: &std::path::Path,
    name: &str,
) -> Result<String, AppError> {
    Ok(redact_url(
        &checked_raw_remote_url(runner, cwd, name).await?,
    ))
}

const BITBUCKET_HOST: &str = "bitbucket.org";
const BITBUCKET_STATIC_USERNAME: &str = "x-bitbucket-api-token-auth";

#[derive(Debug, Clone, PartialEq, Eq)]
struct BitbucketCredentialContext {
    host: String,
    path: String,
    username: String,
}

/// Parse only Bitbucket Cloud HTTPS URLs. Bitbucket Data Center and SSH keep
/// using their existing credential/agent flows and never enter this path.
fn bitbucket_credential_context(url: &str) -> Option<BitbucketCredentialContext> {
    if url.chars().any(char::is_control) {
        return None;
    }
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let (authority, raw_path) = rest.split_once('/')?;
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((userinfo, host)) => (Some(userinfo), host),
        None => (None, authority),
    };
    let host_name = host.split(':').next().unwrap_or("");
    if !host_name.eq_ignore_ascii_case(BITBUCKET_HOST)
        || !(host.eq_ignore_ascii_case(BITBUCKET_HOST)
            || host.eq_ignore_ascii_case("bitbucket.org:443"))
    {
        return None;
    }
    let path = raw_path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches('/');
    if path.is_empty() {
        return None;
    }
    let username = userinfo
        .and_then(|value| value.split(':').next())
        .filter(|value| !value.is_empty())
        .unwrap_or(BITBUCKET_STATIC_USERNAME);
    if username.chars().any(char::is_control) {
        return None;
    }
    Some(BitbucketCredentialContext {
        host: host.to_ascii_lowercase(),
        path: path.to_string(),
        username: username.to_string(),
    })
}

fn with_bitbucket_username(url: &str, mut command: Vec<String>) -> Vec<String> {
    let Some(context) = bitbucket_credential_context(url) else {
        return command;
    };
    let mut argv = vec![
        "-c".to_string(),
        format!("credential.username={}", context.username),
    ];
    argv.append(&mut command);
    argv
}

fn bitbucket_credential_payload(
    context: &BitbucketCredentialContext,
    api_token: &str,
) -> Result<Vec<u8>, AppError> {
    let token = api_token.trim();
    if token.is_empty() || token.len() > 4096 || token.chars().any(char::is_control) {
        return Err(bad_request("Bitbucket API token is empty or invalid"));
    }
    Ok(format!(
        "protocol=https\nhost={}\npath={}\nusername={}\npassword={}\n\n",
        context.host, context.path, context.username, token
    )
    .into_bytes())
}

fn check_remote_name(name: &str) -> Result<(), AppError> {
    if name.is_empty() || name.len() > 250 {
        return Err(bad_request("Remote name is empty or too long"));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(bad_request("Remote name has unsupported characters"));
    }
    Ok(())
}

async fn default_remote(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
) -> Result<String, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if let Some(name) = remote_name_for(runner, &session.worktree_root).await {
        return Ok(name);
    }
    // Fall back to `origin` when it exists.
    let out = runner
        .run(
            &session.worktree_root,
            &["config", "--get", "remote.origin.url"],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("No upstream remote; configure one first"))?;
    if out.success {
        Ok("origin".to_string())
    } else {
        Err(bad_request("No upstream remote; configure one first"))
    }
}

async fn start_job(
    app: &AppHandle,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request_id: &str,
    kind: &'static str,
    argv: Vec<String>,
    log_summary: String,
) -> Result<String, AppError> {
    let record = new_operation(repo_id, request_id, kind);
    let operation_id = record.operation_id.clone();
    let fingerprint = format!("{kind}:{argv:?}");
    registry.job_register(record, &fingerprint)?;
    let task_app = app.clone();
    let task_repo = repo_id.to_string();
    let task_op = operation_id.clone();
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    tauri::async_runtime::spawn(run_remote_job(
        task_app,
        kind,
        task_repo,
        task_op,
        session.worktree_root,
        argv,
        log_summary,
    ));
    Ok(operation_id)
}

// ---- command cores ----

async fn core_fetch(
    app: &AppHandle,
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request_id: &str,
    expected_version: u64,
    remote: Option<&str>,
) -> Result<OperationStarted, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("fetch from remotes"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    let name = match remote {
        Some(name) => {
            check_remote_name(name)?;
            name.to_string()
        }
        None => default_remote(runner, registry, repo_id).await?,
    };
    let url = checked_remote_url(runner, &session.worktree_root, &name).await?;
    let argv = with_bitbucket_username(
        &url,
        vec!["fetch".into(), "--progress".into(), name.clone()],
    );
    let operation_id = start_job(
        app,
        registry,
        repo_id,
        request_id,
        "fetch",
        argv,
        format!("fetch {name} ({url})"),
    )
    .await?;
    Ok(OperationStarted { operation_id })
}

async fn core_pull(
    app: &AppHandle,
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    request_id: &str,
    expected_version: u64,
) -> Result<OperationStarted, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("pull from remotes"));
    }
    if session.version != expected_version {
        return Err(stale_state());
    }
    // Clean worktree precondition; MVP never autostashes or rebases.
    let parsed = crate::git::read_status(runner, &session.worktree_root)
        .await
        .map_err(|_| bad_request("Cannot read the worktree"))?;
    let counts = parsed.counts();
    if counts.staged != 0 || counts.unstaged != 0 || counts.conflicted != 0 {
        return Err(AppError::new(
            ErrorCode::DIRTY_WORKTREE,
            "Stash or commit your changes before pulling",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let upstream = resolve_upstream(runner, &session.worktree_root)
        .await
        .map_err(|_| bad_request("Cannot read the upstream"))?;
    let upstream =
        upstream.ok_or_else(|| bad_request("No upstream configured for the current branch"))?;
    // Backend enforces ff-only regardless of repo config.
    let argv = with_bitbucket_username(
        &upstream.url,
        vec!["pull".into(), "--ff-only".into(), "--progress".into()],
    );
    let operation_id = start_job(
        app,
        registry,
        repo_id,
        request_id,
        "pull",
        argv,
        format!("pull --ff-only {} ({})", upstream.remote, upstream.url),
    )
    .await?;
    Ok(OperationStarted { operation_id })
}

async fn core_push(
    app: &AppHandle,
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    request: &PushRequest,
) -> Result<OperationStarted, AppError> {
    let (argv, summary) = prepare_push(runner, registry, request).await?;
    let operation_id = start_job(
        app,
        registry,
        &request.repo_id,
        &request.request_id,
        "push",
        argv,
        summary,
    )
    .await?;
    Ok(OperationStarted { operation_id })
}

// Shared with real-Git regression tests; background execution must use this
// exact plan so repository push defaults cannot broaden the requested write.
async fn prepare_push(
    runner: &GitRunner,
    registry: &RepoRegistry,
    request: &PushRequest,
) -> Result<(Vec<String>, String), AppError> {
    let repo_id = request.repo_id.as_str();
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    if session.trust != TrustState::Trusted {
        return Err(trust_required("push to remotes"));
    }
    if session.version != request.expected_version {
        return Err(stale_state());
    }
    let head_name = runner
        .run(
            &session.worktree_root,
            &["symbolic-ref", "-q", "--short", "HEAD"],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("Cannot read HEAD"))?;
    if !head_name.success {
        return Err(bad_request(
            "Detached HEAD cannot be pushed; create a branch first",
        ));
    }
    let branch = String::from_utf8_lossy(&head_name.stdout)
        .trim()
        .to_string();
    let upstream = resolve_upstream(runner, &session.worktree_root)
        .await
        .map_err(|_| bad_request("Cannot read the upstream"))?;
    let name = match (request.remote.as_deref(), &upstream) {
        (Some(name), _) => {
            check_remote_name(name)?;
            name.to_string()
        }
        (None, Some(up)) => up.remote.clone(),
        (None, None) if request.set_upstream => default_remote(runner, registry, repo_id).await?,
        (None, None) => {
            return Err(bad_request(
                "No upstream configured; choose a remote to set it explicitly",
            ));
        }
    };
    check_remote_name(&name)?;
    let source = runner
        .run(
            &session.worktree_root,
            &["rev-parse", "--verify", "HEAD^{commit}"],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("Cannot read HEAD"))?;
    if !source.success {
        return Err(bad_request("Create a commit before pushing"));
    }
    let source_oid = String::from_utf8_lossy(&source.stdout).trim().to_string();
    let target = if upstream.as_ref().is_some_and(|up| up.remote == name) {
        let out = runner
            .run(
                &session.worktree_root,
                &["config", "--get-all", &format!("branch.{branch}.merge")],
                crate::git::READ_TIMEOUT,
            )
            .await
            .map_err(|_| bad_request("Cannot read the upstream branch"))?;
        let refs = String::from_utf8_lossy(&out.stdout);
        let refs: Vec<_> = refs.lines().collect();
        if !out.success || refs.len() != 1 || !refs[0].starts_with("refs/heads/") {
            return Err(bad_request("Push requires exactly one upstream branch"));
        }
        refs[0].to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let valid_target = runner
        .run(
            &session.worktree_root,
            &["check-ref-format", &target],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("Cannot validate the push branch"))?;
    if !valid_target.success {
        return Err(bad_request("The upstream branch is invalid"));
    }

    // Resolve pushurl and URL rewrites too; validate the actual destination
    // and refuse multi-destination pushes rather than logging only one URL.
    let urls = runner
        .run(
            &session.worktree_root,
            &["remote", "get-url", "--push", "--all", "--", &name],
            crate::git::READ_TIMEOUT,
        )
        .await
        .map_err(|_| bad_request("Cannot read the push URL"))?;
    if !urls.success {
        return Err(bad_request("Cannot read the push URL"));
    }
    let urls_text = String::from_utf8_lossy(&urls.stdout);
    let urls: Vec<_> = urls_text.lines().collect();
    if urls.len() != 1 || urls[0].is_empty() {
        return Err(bad_request("Push requires exactly one destination URL"));
    }
    validate_remote_url(urls[0]).map_err(|_| bad_request("The push URL is not supported"))?;
    let url = redact_url(urls[0]);
    let mut argv = vec![
        "-c".into(),
        format!("remote.{name}.mirror=false"),
        "push".into(),
        "--progress".into(),
    ];
    argv.push(if request.force {
        "--force-with-lease".into()
    } else {
        "--no-force".into()
    });
    argv.extend(["--no-follow-tags".into(), "--recurse-submodules=no".into()]);
    let set_upstream = upstream.is_none() && request.set_upstream;
    if set_upstream {
        argv.push("--set-upstream".into());
    }
    argv.extend(["--".into(), name.clone(), format!("HEAD:{target}")]);
    let summary = format!(
        "push {}{}{name} HEAD:{target} ({url} {})",
        if request.force {
            "--force-with-lease "
        } else {
            ""
        },
        if set_upstream { "-u " } else { "" },
        short_oid(&source_oid)
    );
    Ok((with_bitbucket_username(urls[0], argv), summary))
}

fn short_oid(oid: &str) -> String {
    oid.chars().take(12).collect()
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteContext {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub remote: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub remote: Option<String>,
    #[serde(default)]
    pub set_upstream: bool,
    /// Force-push with lease: overwrites the remote tip only when it still
    /// matches the last fetched state, so unknown remote work is never lost.
    #[serde(default)]
    pub force: bool,
}

/// Secret-bearing request. Deliberately does not implement `Debug` so the
/// API token cannot be emitted by incidental request logging.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketConnectRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub remote: Option<String>,
    pub api_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    #[serde(default)]
    pub cursor: u64,
}

// ---- Commands ----

#[tauri::command]
pub async fn remote_status(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RemoteContext,
) -> Result<ApiResult<RemoteStatus>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        let session = registry
            .get(&request.repo_id)
            .ok_or_else(session_missing)?
            .clone();
        let (upstream, mut url) = upstream_summary(&runner, &session.worktree_root).await;
        let mut name = match &upstream {
            Some(_) => remote_name_for(&runner, &session.worktree_root).await,
            None => None,
        };
        // A repository may have `origin` but no branch upstream yet. Surface
        // that remote so first-push auth and set-upstream recovery are usable.
        if name.is_none() {
            if let Ok(fallback) = default_remote(&runner, &registry, &request.repo_id).await {
                url = checked_remote_url(&runner, &session.worktree_root, &fallback)
                    .await
                    .ok();
                name = Some(fallback);
            }
        }
        Ok::<RemoteStatus, AppError>(RemoteStatus {
            remote_name: name,
            url,
            upstream_ref: upstream.as_ref().map(|u| u.ref_id.clone()),
            ahead: upstream.as_ref().map(|u| u.ahead),
            behind: upstream.as_ref().map(|u| u.behind),
            last_fetch_at: registry.fetch_time(&request.repo_id).map(epoch_to_iso8601),
        })
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

/// Save a scoped Bitbucket Cloud API token through the user's configured Git
/// credential helper. The token travels over IPC once, then only through the
/// child process stdin; it is never placed in argv, environment, URL or logs.
#[tauri::command]
pub async fn bitbucket_connect(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: BitbucketConnectRequest,
) -> Result<ApiResult<BitbucketConnectionResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        let session = registry
            .get(&request.repo_id)
            .ok_or_else(session_missing)?
            .clone();
        if session.trust != TrustState::Trusted {
            return Err(trust_required("save a Bitbucket credential"));
        }
        if session.version != request.expected_version {
            return Err(stale_state());
        }
        let remote_name = match request.remote.as_deref() {
            Some(name) => {
                check_remote_name(name)?;
                name.to_string()
            }
            None => default_remote(&runner, &registry, &request.repo_id).await?,
        };
        let raw_url =
            checked_raw_remote_url(&runner, &session.worktree_root, &remote_name).await?;
        let context = bitbucket_credential_context(&raw_url).ok_or_else(|| {
            AppError::new(
                ErrorCode::UNSUPPORTED,
                "Bitbucket token integration requires an HTTPS bitbucket.org remote; SSH remotes use the SSH agent",
                RecoveryAction::InspectState,
                false,
            )
        })?;
        let helper = runner
            .run(
                &session.worktree_root,
                &[
                    "config",
                    "--get-urlmatch",
                    "credential.helper",
                    "https://bitbucket.org",
                ],
                crate::git::READ_TIMEOUT,
            )
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::IO_ERROR,
                    "Octopus could not inspect the Git credential helper",
                    RecoveryAction::ConfigureGit,
                    false,
                )
            })?;
        if !helper.success
            || String::from_utf8_lossy(&helper.stdout)
                .lines()
                .all(|line| line.trim().is_empty())
        {
            return Err(AppError::new(
                ErrorCode::UNSUPPORTED,
                "No Git credential helper is configured for Bitbucket. Configure a secure Git credential helper, then retry",
                RecoveryAction::ConfigureGit,
                false,
            ));
        }
        let payload = bitbucket_credential_payload(&context, &request.api_token)?;
        let approved = runner
            .run_with_stdin(
                &session.worktree_root,
                &["credential", "approve"],
                &payload,
                crate::git::WRITE_TIMEOUT,
            )
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorCode::IO_ERROR,
                    "Git credential helper could not save the Bitbucket token",
                    RecoveryAction::ConfigureGit,
                    false,
                )
            })?;
        if !approved.success {
            return Err(AppError::new(
                ErrorCode::IO_ERROR,
                "Git credential helper rejected the Bitbucket token",
                RecoveryAction::ConfigureGit,
                false,
            ));
        }
        Ok::<BitbucketConnectionResult, AppError>(BitbucketConnectionResult {
            remote_name,
            username: context.username,
            credential_saved: true,
        })
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn remote_fetch(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: FetchRequest,
) -> Result<ApiResult<OperationStarted>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_fetch(
            &app,
            &runner,
            &mut registry,
            &request.repo_id,
            &request_id,
            request.expected_version,
            request.remote.as_deref(),
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
pub async fn remote_pull(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: PullRequest,
) -> Result<ApiResult<OperationStarted>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_pull(
            &app,
            &runner,
            &mut registry,
            &request.repo_id,
            &request_id,
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
pub async fn remote_push(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: PushRequest,
) -> Result<ApiResult<OperationStarted>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        // Normal push can never force: the flag is pinned off so a crafted
        // request cannot broaden this endpoint into a history rewrite.
        let mut normal = request;
        normal.force = false;
        core_push(&app, &runner, &mut registry, &normal).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

/// Force-push variant of [`remote_push`]: same planning, trust, version and
/// single-destination guards, but the push runs with `--force-with-lease` so
/// a rewritten local history can overwrite the remote tip it already knows.
#[tauri::command]
pub async fn remote_push_force(
    app: AppHandle,
    registry: State<'_, Mutex<RepoRegistry>>,
    request: PushRequest,
) -> Result<ApiResult<OperationStarted>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        let mut forced = request;
        forced.force = true;
        core_push(&app, &runner, &mut registry, &forced).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn operation_log(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: LogRequest,
) -> Result<ApiResult<OperationLogPage>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let registry = registry.lock().await;
        if registry.get(&request.repo_id).is_none() {
            return Err(session_missing());
        }
        let (entries, next_cursor) = registry.log_list(&request.repo_id, request.cursor);
        Ok::<OperationLogPage, AppError>(OperationLogPage {
            entries,
            next_cursor,
        })
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

/// Create a pull (merge) request on the hosting provider.
///
/// Multi-provider by remote host: `github.com`, `bitbucket.org`,
/// `gitlab.com`. Credentials are never stored by Octopus for this flow: they
/// are read back from the user's configured Git credential helper
/// (`git credential fill`, non-interactive) and travel only in the
/// Authorization header of a single TLS request. Nothing secret is logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrProvider {
    Bitbucket,
    GitHub,
    GitLab,
}

impl PrProvider {
    fn name(self) -> &'static str {
        match self {
            PrProvider::Bitbucket => "bitbucket",
            PrProvider::GitHub => "github",
            PrProvider::GitLab => "gitlab",
        }
    }
}

/// Owner/workspace + repository coordinates parsed from a remote URL.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PrCoords {
    provider: PrProvider,
    /// API base without trailing slash (`https://api.github.com`,
    /// `https://api.bitbucket.org/2.0`, `https://<host>/api/v4`).
    api_base: String,
    /// `owner`, `workspace`, or full subgroup path (`group/sub`).
    namespace: String,
    repo: String,
}

/// Split `owner/repo(.git)`-style paths; rejects empty segments.
fn split_namespace_repo(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim_matches('/');
    let (namespace, repo) = trimmed.rsplit_once('/')?;
    if namespace.is_empty() || repo.is_empty() {
        return None;
    }
    if namespace.split('/').any(str::is_empty) {
        return None;
    }
    Some((namespace.to_string(), repo.to_string()))
}

/// Normalize a remote URL into provider coordinates. Accepts HTTPS and
/// `git@host:path` SSH forms; SSH remotes still need an HTTPS credential
/// saved for the host (see [`pr_credential`]).
fn detect_pr_provider(remote_url: &str) -> Option<PrCoords> {
    // Config output trails a newline; trim first, then reject controls.
    let url = remote_url.trim();
    if url.chars().any(char::is_control) {
        return None;
    }
    // Scheme URLs first: an `https://user@host/...` remote must never fall
    // into the SSH branch below (its userinfo contains `/` and `:`).
    if let Some((scheme, rest)) = url.split_once("://") {
        if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")) {
            return None;
        }
        // Strip optional userinfo.
        let rest = rest
            .rsplit_once('@')
            .map(|(_, after)| after)
            .unwrap_or(rest);
        let (authority, mut path) = rest.split_once('/')?;
        let host = authority.split(':').next().unwrap_or("");
        if host.is_empty() {
            return None;
        }
        path = path
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .trim_matches('/');
        path = path.strip_suffix(".git").unwrap_or(path);
        return coords_for_host(host, path, true);
    }
    // SSH short form: `git@github.com:owner/repo.git`.
    let after_at = url.split_once('@')?;
    if after_at.0.is_empty() || after_at.0.contains('/') || after_at.0.contains(':') {
        return None;
    }
    let (host, mut path) = after_at.1.split_once(':')?;
    if host.is_empty() || path.is_empty() {
        return None;
    }
    path = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches('/');
    path = path.strip_suffix(".git").unwrap_or(path);
    coords_for_host(host, path, false)
}

fn coords_for_host(host: &str, path: &str, _https: bool) -> Option<PrCoords> {
    if path.is_empty() {
        return None;
    }
    let (namespace, repo) = split_namespace_repo(path)?;
    if host.eq_ignore_ascii_case("github.com") {
        if namespace.contains('/') {
            return None;
        }
        Some(PrCoords {
            provider: PrProvider::GitHub,
            api_base: "https://api.github.com".to_string(),
            namespace,
            repo,
        })
    } else if host.eq_ignore_ascii_case("bitbucket.org") {
        if namespace.contains('/') {
            return None;
        }
        Some(PrCoords {
            provider: PrProvider::Bitbucket,
            api_base: "https://api.bitbucket.org/2.0".to_string(),
            namespace,
            repo,
        })
    } else if host.eq_ignore_ascii_case("gitlab.com") {
        Some(PrCoords {
            provider: PrProvider::GitLab,
            api_base: "https://gitlab.com/api/v4".to_string(),
            namespace,
            repo,
        })
    } else {
        None
    }
}

/// Percent-encode a GitLab project path (`group/sub/repo` -> `group%2Fsub%2Frepo`).
fn gitlab_encode_project(namespace: &str, repo: &str) -> String {
    fn encode(segment: &str) -> String {
        let mut out = String::with_capacity(segment.len());
        for byte in segment.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                out.push(byte as char);
            } else {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
        out
    }
    namespace
        .split('/')
        .chain(std::iter::once(repo))
        .map(encode)
        .collect::<Vec<_>>()
        .join("%2F")
}

fn check_pr_text(value: &str, field: &str, max_len: usize) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        return Err(bad_request(format!("{field} is empty or invalid")));
    }
    Ok(trimmed.to_string())
}

async fn check_branch_name(
    runner: &GitRunner,
    cwd: &std::path::Path,
    name: &str,
) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 250 || trimmed.chars().any(char::is_control) {
        return Err(bad_request("Branch name is empty or invalid"));
    }
    let out = runner
        .run(
            cwd,
            &["check-ref-format", "--branch", trimmed],
            std::time::Duration::from_secs(15),
        )
        .await
        .map_err(|_| bad_request("Could not validate the branch name"))?;
    if !out.success {
        return Err(bad_request("Branch name is not a valid Git branch name"));
    }
    Ok(trimmed.to_string())
}

/// Read a saved credential back via `git credential fill` (non-interactive;
/// never prompts). Returns `(username, password)`.
async fn pr_credential(
    runner: &GitRunner,
    cwd: &std::path::Path,
    host: &str,
) -> Result<(String, String), AppError> {
    let input = format!("protocol=https\nhost={host}\n\n");
    let out = runner
        .run_with_stdin(
            cwd,
            &["credential", "fill"],
            input.as_bytes(),
            std::time::Duration::from_secs(15),
        )
        .await
        .map_err(|e| match e {
            crate::git::runner::RunError::TimedOut => AppError::new(
                ErrorCode::TIMEOUT,
                "Reading the saved credential timed out",
                RecoveryAction::Refresh,
                true,
            ),
            _ => AppError::new(
                ErrorCode::AUTH_REQUIRED,
                "No saved credential for this host. Save one first (Bitbucket: Connect in the push flow; GitHub/GitLab: store a token with your Git credential helper).",
                RecoveryAction::Authenticate,
                false,
            ),
        })?;
    if !out.success {
        return Err(AppError::new(
            ErrorCode::AUTH_REQUIRED,
            "No saved credential for this host. Save one first (Bitbucket: Connect in the push flow; GitHub/GitLab: store a token with your Git credential helper).",
            RecoveryAction::Authenticate,
            false,
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut username = String::new();
    let mut password = String::new();
    for line in stdout.lines() {
        if let Some(value) = line.strip_prefix("username=") {
            username = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("password=") {
            password = value.trim().to_string();
        }
    }
    if password.is_empty()
        || password.len() > 4096
        || password.chars().any(char::is_control)
        || username.len() > 1024
        || username.chars().any(char::is_control)
    {
        return Err(AppError::new(
            ErrorCode::AUTH_REQUIRED,
            "No saved credential for this host. Save one first (Bitbucket: Connect in the push flow; GitHub/GitLab: store a token with your Git credential helper).",
            RecoveryAction::Authenticate,
            false,
        ));
    }
    Ok((username, password))
}

fn pr_request_json(
    provider: PrProvider,
    title: &str,
    description: &str,
    source: &str,
    target: &str,
) -> serde_json::Value {
    match provider {
        PrProvider::GitHub => serde_json::json!({
            "title": title,
            "head": source,
            "base": target,
            "body": description,
        }),
        PrProvider::Bitbucket => serde_json::json!({
            "title": title,
            "description": description,
            "source": { "branch": { "name": source } },
            "destination": { "branch": { "name": target } },
        }),
        PrProvider::GitLab => serde_json::json!({
            "title": title,
            "description": description,
            "source_branch": source,
            "target_branch": target,
        }),
    }
}

fn pr_endpoint(coords: &PrCoords) -> String {
    match coords.provider {
        PrProvider::GitHub => format!(
            "{}/repos/{}/{}/pulls",
            coords.api_base, coords.namespace, coords.repo
        ),
        PrProvider::Bitbucket => format!(
            "{}/repositories/{}/{}/pullrequests",
            coords.api_base, coords.namespace, coords.repo
        ),
        PrProvider::GitLab => format!(
            "{}/projects/{}/merge_requests",
            coords.api_base,
            gitlab_encode_project(&coords.namespace, &coords.repo)
        ),
    }
}

/// Pull the PR URL + reference out of a provider response (size-bounded).
fn pr_parse_response(provider: PrProvider, body: &[u8]) -> Result<(String, String), AppError> {
    if body.len() > 1024 * 1024 {
        return Err(AppError::new(
            ErrorCode::OUTPUT_LIMIT,
            "Provider response exceeded the safety bound",
            RecoveryAction::RetryRead,
            false,
        ));
    }
    let json: serde_json::Value = serde_json::from_slice(body).map_err(|_| {
        AppError::new(
            ErrorCode::NETWORK_ERROR,
            "Provider returned an unreadable response",
            RecoveryAction::Refresh,
            false,
        )
    })?;
    let invalid = || {
        AppError::new(
            ErrorCode::NETWORK_ERROR,
            "Provider returned an unreadable response",
            RecoveryAction::Refresh,
            false,
        )
    };
    let (url, reference) = match provider {
        PrProvider::GitHub => (
            json.get("html_url").and_then(|v| v.as_str()).unwrap_or(""),
            json.get("number")
                .and_then(|v| v.as_u64())
                .map(|n| format!("#{n}"))
                .unwrap_or_default(),
        ),
        PrProvider::Bitbucket => (
            json.pointer("/links/html/href")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            json.get("id")
                .and_then(|v| v.as_u64())
                .map(|n| format!("#{n}"))
                .unwrap_or_default(),
        ),
        PrProvider::GitLab => (
            json.get("web_url").and_then(|v| v.as_str()).unwrap_or(""),
            json.get("iid")
                .and_then(|v| v.as_u64())
                .map(|n| format!("!{n}"))
                .unwrap_or_default(),
        ),
    };
    if url.is_empty() || reference.is_empty() || !url.starts_with("https://") {
        return Err(invalid());
    }
    Ok((url.to_string(), reference))
}

fn pr_http_error(status: reqwest::StatusCode, provider: PrProvider, body: &[u8]) -> AppError {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return AppError::new(
            ErrorCode::AUTH_REQUIRED,
            "The provider rejected the saved credential. Update it, then retry.",
            RecoveryAction::Authenticate,
            false,
        );
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            "Repository or branch not found on the provider. Push the branches first.",
            RecoveryAction::InspectState,
            false,
        );
    }
    if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        || status == reqwest::StatusCode::BAD_REQUEST
    {
        let hint = serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|json| {
                json.get("message")
                    .or_else(|| json.get("error"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .filter(|message| {
                !message.is_empty()
                    && message.len() <= 300
                    && !message.chars().any(char::is_control)
            })
            .map(|message| format!(" Provider says: {message}"))
            .unwrap_or_default();
        return AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            format!(
                "The provider refused the {} (maybe it already exists).{hint}",
                match provider {
                    PrProvider::GitLab => "merge request",
                    _ => "pull request",
                }
            ),
            RecoveryAction::InspectState,
            false,
        );
    }
    AppError::new(
        ErrorCode::NETWORK_ERROR,
        format!("Provider request failed (HTTP {})", status.as_u16()),
        RecoveryAction::Refresh,
        false,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCreateRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub expected_version: u64,
    pub remote: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullRequestResult {
    pub provider: String,
    pub url: String,
    pub reference: String,
}

async fn core_pull_request_create(
    runner: &GitRunner,
    registry: &RepoRegistry,
    http: &reqwest::Client,
    api_base_override: Option<&str>,
    request: &PullRequestCreateRequest,
    cwd: &std::path::Path,
) -> Result<PullRequestResult, AppError> {
    let remote_name = match request.remote.as_deref() {
        Some(name) => {
            check_remote_name(name)?;
            name.to_string()
        }
        None => default_remote(runner, registry, &request.repo_id).await?,
    };
    let raw_url = checked_raw_remote_url(runner, cwd, &remote_name).await?;
    let mut coords = detect_pr_provider(&raw_url).ok_or_else(|| {
        AppError::new(
            ErrorCode::UNSUPPORTED,
            "Pull requests support github.com, bitbucket.org and gitlab.com remotes",
            RecoveryAction::InspectState,
            false,
        )
    })?;
    if let Some(base) = api_base_override {
        coords.api_base = base.trim_end_matches('/').to_string();
    }
    let source = check_branch_name(runner, cwd, &request.source_branch).await?;
    let target = check_branch_name(runner, cwd, &request.target_branch).await?;
    if source == target {
        return Err(bad_request("Source and target branches must differ"));
    }
    let title = check_pr_text(&request.title, "Title", 512)?;
    let description = if request.description.trim().is_empty() {
        String::new()
    } else {
        check_pr_text(&request.description, "Description", 65536)?
    };
    let host = raw_url_host(&raw_url).unwrap_or_default();
    let (_username, password) = pr_credential(runner, cwd, &host).await?;
    let endpoint = pr_endpoint(&coords);
    let payload = pr_request_json(coords.provider, &title, &description, &source, &target);
    let mut builder = http
        .post(&endpoint)
        .header("User-Agent", "Octopus/0.1")
        .header("Accept", "application/json")
        .json(&payload);
    builder = match coords.provider {
        PrProvider::GitLab => builder.header("PRIVATE-TOKEN", password),
        _ => builder.bearer_auth(password),
    };
    let response = builder.send().await.map_err(|e| {
        if e.is_timeout() {
            AppError::new(
                ErrorCode::TIMEOUT,
                "Provider request timed out",
                RecoveryAction::Refresh,
                false,
            )
        } else {
            AppError::new(
                ErrorCode::NETWORK_ERROR,
                "Could not reach the provider",
                RecoveryAction::Refresh,
                false,
            )
        }
    })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|_| {
        AppError::new(
            ErrorCode::NETWORK_ERROR,
            "Could not read the provider response",
            RecoveryAction::Refresh,
            false,
        )
    })?;
    if !status.is_success() {
        return Err(pr_http_error(status, coords.provider, &body));
    }
    let (url, reference) = pr_parse_response(coords.provider, &body)?;
    Ok(PullRequestResult {
        provider: coords.provider.name().to_string(),
        url,
        reference,
    })
}

/// Host part of an HTTPS or SSH remote URL, lowercased.
fn raw_url_host(remote_url: &str) -> Option<String> {
    if let Some(after_at) = remote_url.split_once('@') {
        let (host, _) = after_at.1.split_once(':')?;
        if host.is_empty() {
            return None;
        }
        return Some(host.to_ascii_lowercase());
    }
    let (_, rest) = remote_url.split_once("://")?;
    let rest = rest
        .rsplit_once('@')
        .map(|(_, after)| after)
        .unwrap_or(rest);
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority.split(':').next().unwrap_or("");
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// Create a pull (merge) request on the hosting provider. External write:
/// requires a trusted repository and a matching session version.
#[tauri::command]
pub async fn pull_request_create(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: PullRequestCreateRequest,
) -> Result<ApiResult<PullRequestResult>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        let session = registry
            .get(&request.repo_id)
            .ok_or_else(session_missing)?
            .clone();
        if session.trust != TrustState::Trusted {
            return Err(trust_required("create a pull request"));
        }
        if session.version != request.expected_version {
            return Err(stale_state());
        }
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| {
                AppError::new(
                    ErrorCode::NETWORK_ERROR,
                    "Could not start the provider request",
                    RecoveryAction::Refresh,
                    true,
                )
            })?;
        core_pull_request_create(
            &runner,
            &registry,
            &http,
            None,
            &request,
            &session.worktree_root,
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
        bitbucket_connect, epoch_to_iso8601, operation_log, pull_request_create, remote_fetch,
        remote_pull, remote_push, remote_push_force, remote_status, BitbucketConnectRequest,
        FetchRequest, LogRequest, PullRequest, PullRequestCreateRequest, PullRequestResult,
        PushRequest, RemoteContext,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::commit::tests::{git, open_repo, temp_repo};
    use std::sync::atomic::AtomicBool;

    fn push_fixture(
        label: &str,
    ) -> (
        std::path::PathBuf,
        std::path::PathBuf,
        RepoRegistry,
        PushRequest,
    ) {
        let (root, repo) = temp_repo(&format!("push-{label}-{}", uuid::Uuid::new_v4()));
        git(&repo, &["config", "user.name", "Fixture"]);
        git(&repo, &["config", "user.email", "fixture@example.test"]);
        git(&repo, &["config", "commit.gpgSign", "false"]);
        std::fs::write(repo.join("base"), "base\n").unwrap();
        git(&repo, &["add", "base"]);
        git(&repo, &["commit", "-m", "base"]);
        git(&repo, &["branch", "side"]);
        for name in ["origin", "other"] {
            let path = root.join(format!("{name}.git"));
            git(&root, &["init", "--bare", path.to_str().unwrap()]);
            git(&repo, &["remote", "add", name, path.to_str().unwrap()]);
            git(&repo, &["push", name, "main", "side"]);
        }
        git(&repo, &["branch", "--set-upstream-to=origin/main", "main"]);
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        let request = PushRequest {
            request_id: "push-test".into(),
            expected_version: registry.get(&repo_id).unwrap().version,
            repo_id,
            remote: None,
            set_upstream: false,
            force: false,
        };
        (root, repo, registry, request)
    }

    fn git_text(repo: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn advance(repo: &std::path::Path, name: &str) {
        std::fs::write(repo.join(name), name).unwrap();
        git(repo, &["add", name]);
        git(repo, &["commit", "-m", name]);
    }

    async fn run_push_plan(
        repo: &std::path::Path,
        registry: &RepoRegistry,
        request: &PushRequest,
    ) -> JobOutcome {
        let runner = git_runner().unwrap();
        let (args, _) = prepare_push(&runner, registry, request).await.unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        execute_remote("git", &args, repo, preset(), tx).await
    }

    #[tokio::test]
    async fn push_plan_limits_configured_matching_mirror_and_tags_to_one_branch() {
        let (root, repo, registry, request) = push_fixture("matching");
        let origin = root.join("origin.git");
        let initial = git_text(&origin, &["rev-parse", "main"]);
        git(&origin, &["branch", "remote-only", "main"]);
        git(&repo, &["switch", "side"]);
        advance(&repo, "side-change");
        git(&repo, &["switch", "main"]);
        advance(&repo, "main-change");
        git(
            &repo,
            &[
                "-c",
                "tag.gpgSign=false",
                "tag",
                "-a",
                "local-tag",
                "-m",
                "local tag",
            ],
        );
        git(&repo, &["config", "push.default", "matching"]);
        git(
            &repo,
            &["config", "remote.origin.push", "+refs/heads/*:refs/heads/*"],
        );
        git(&repo, &["config", "remote.origin.mirror", "true"]);
        git(&repo, &["config", "push.followTags", "true"]);
        git(&repo, &["config", "branch.main.pushRemote", "other"]);
        git(&repo, &["config", "remote.pushDefault", "other"]);
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(
            git_text(&origin, &["rev-parse", "main"]),
            git_text(&repo, &["rev-parse", "HEAD"])
        );
        assert_eq!(git_text(&origin, &["rev-parse", "side"]), initial);
        assert_eq!(git_text(&origin, &["rev-parse", "remote-only"]), initial);
        assert_eq!(
            git_text(&root.join("other.git"), &["rev-parse", "main"]),
            initial
        );
        assert!(git_text(&origin, &["tag", "--list"]).is_empty());
    }

    #[tokio::test]
    async fn push_rejected_after_amend_maps_diverged_then_force_pushes() {
        let (root, repo, registry, mut request) = push_fixture("rejected");
        let origin = root.join("origin.git");
        advance(&repo, "local-work");
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        let pushed = git_text(&repo, &["rev-parse", "HEAD"]);
        // Rewrite the pushed tip: a normal push must now fail as diverged.
        git(&repo, &["commit", "--amend", "-m", "rewritten"]);
        match run_push_plan(&repo, &registry, &request).await {
            JobOutcome::Failed(error) => assert_eq!(error.code, ErrorCode::DIVERGED),
            other => panic!("expected diverged push failure, got {other:?}"),
        }
        assert_eq!(git_text(&origin, &["rev-parse", "main"]), pushed);
        // Force-with-lease overwrites the known tip and lands the rewrite.
        request.force = true;
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(
            git_text(&origin, &["rev-parse", "main"]),
            git_text(&repo, &["rev-parse", "HEAD"])
        );
    }

    #[tokio::test]
    async fn prepare_push_force_uses_lease_and_marks_summary() {
        let (_root, _repo, registry, mut request) = push_fixture("lease-plan");
        let runner = git_runner().unwrap();
        let (normal_argv, normal_summary) =
            prepare_push(&runner, &registry, &request).await.unwrap();
        assert!(normal_argv.contains(&"--no-force".to_string()));
        assert!(!normal_summary.contains("force"));
        request.force = true;
        let (force_argv, force_summary) = prepare_push(&runner, &registry, &request).await.unwrap();
        assert!(force_argv.contains(&"--force-with-lease".to_string()));
        assert!(!force_argv.iter().any(|arg| arg == "--no-force"));
        assert!(force_summary.contains("--force-with-lease"));
    }

    #[test]
    fn network_error_rejected_push_maps_diverged() {
        let stderr = "To /tmp/origin.git\n ! [rejected]        main -> main (non-fast-forward)\nerror: failed to push some refs to '/tmp/origin.git'\nhint: Updates were rejected because the tip of your current branch is behind";
        assert_eq!(network_error(stderr).code, ErrorCode::DIVERGED);
        let stale_lease =
            "! [rejected]        main -> main (stale info)\nerror: failed to push some refs";
        assert_eq!(network_error(stale_lease).code, ErrorCode::DIVERGED);
        // Transport faults still classify as before.
        assert_eq!(
            network_error("fatal: could not resolve host example.invalid").code,
            ErrorCode::OFFLINE
        );
    }

    #[test]
    fn push_request_force_defaults_off() {
        let request: PushRequest = serde_json::from_value(serde_json::json!({
            "requestId": "r1",
            "repoId": "repo",
            "expectedVersion": 3,
            "remote": null,
            "setUpstream": true
        }))
        .unwrap();
        assert!(!request.force);
        assert!(request.set_upstream);
    }

    #[tokio::test]
    async fn push_plan_honors_explicit_remote_and_differently_named_upstream() {
        let (root, repo, registry, mut request) = push_fixture("destinations");
        let origin = root.join("origin.git");
        let initial = git_text(&origin, &["rev-parse", "main"]);
        git(&repo, &["push", "origin", "HEAD:refs/heads/release"]);
        git(
            &repo,
            &["config", "branch.main.merge", "refs/heads/release"],
        );
        advance(&repo, "new-content");
        let head = git_text(&repo, &["rev-parse", "HEAD"]);
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(git_text(&origin, &["rev-parse", "release"]), head);
        assert_eq!(git_text(&origin, &["rev-parse", "main"]), initial);
        request.remote = Some("other".into());
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(
            git_text(&root.join("other.git"), &["rev-parse", "main"]),
            head
        );
    }

    #[tokio::test]
    async fn push_plan_can_set_upstream_on_first_push() {
        let (_root, repo, registry, mut request) = push_fixture("first");
        git(&repo, &["switch", "-c", "new-branch"]);
        advance(&repo, "new-content");
        request.set_upstream = true;
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(
            git_text(&repo, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
            "origin/new-branch"
        );
    }

    #[tokio::test]
    async fn push_plan_never_forces_a_diverged_remote() {
        let (root, repo, registry, request) = push_fixture("diverged");
        advance(&repo, "local-change");
        git(&repo, &["switch", "-c", "remote-tip", "HEAD~1"]);
        advance(&repo, "remote-change");
        git(&repo, &["push", "origin", "HEAD:refs/heads/main"]);
        let remote_head = git_text(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["switch", "main"]);
        git(
            &repo,
            &[
                "config",
                "remote.origin.push",
                "+refs/heads/main:refs/heads/main",
            ],
        );
        git(&repo, &["config", "remote.origin.mirror", "true"]);
        assert!(matches!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Failed(_)
        ));
        assert_eq!(
            git_text(&root.join("origin.git"), &["rev-parse", "main"]),
            remote_head
        );
    }

    #[tokio::test]
    async fn push_plan_validates_pushurl_and_rejects_multiple_destinations() {
        let (root, repo, registry, request) = push_fixture("urls");
        let other = root.join("other.git");
        git(
            &repo,
            &["config", "remote.origin.pushurl", other.to_str().unwrap()],
        );
        advance(&repo, "only-other");
        let runner = git_runner().unwrap();
        let (_, summary) = prepare_push(&runner, &registry, &request).await.unwrap();
        assert!(summary.contains(other.to_str().unwrap()));
        assert_eq!(
            run_push_plan(&repo, &registry, &request).await,
            JobOutcome::Ok
        );
        assert_eq!(
            git_text(&other, &["rev-parse", "main"]),
            git_text(&repo, &["rev-parse", "HEAD"])
        );
        git(
            &repo,
            &[
                "config",
                "--add",
                "remote.origin.pushurl",
                root.join("origin.git").to_str().unwrap(),
            ],
        );
        assert!(prepare_push(&runner, &registry, &request).await.is_err());
        git(
            &repo,
            &[
                "config",
                "--replace-all",
                "remote.origin.pushurl",
                "ext::blocked",
            ],
        );
        assert!(prepare_push(&runner, &registry, &request).await.is_err());
    }

    fn preset() -> std::sync::Arc<AtomicBool> {
        std::sync::Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn bitbucket_context_accepts_only_cloud_https_and_never_uses_a_password_url() {
        let plain = bitbucket_credential_context(
            "https://bitbucket.org/acme/widgets.git?ignored=1#fragment",
        )
        .expect("Bitbucket HTTPS URL");
        assert_eq!(plain.host, "bitbucket.org");
        assert_eq!(plain.path, "acme/widgets.git");
        assert_eq!(plain.username, BITBUCKET_STATIC_USERNAME);

        let named = bitbucket_credential_context(
            "https://CaseSensitiveUser@bitbucket.org/acme/widgets.git",
        )
        .expect("Bitbucket HTTPS URL with username");
        assert_eq!(named.username, "CaseSensitiveUser");
        assert!(bitbucket_credential_context("git@bitbucket.org:acme/widgets.git").is_none());
        assert!(
            bitbucket_credential_context("https://bitbucket.org.evil.test/acme/widgets.git")
                .is_none()
        );
    }

    #[test]
    fn bitbucket_token_is_stdin_only_and_username_override_contains_no_secret() {
        let context = bitbucket_credential_context("https://bitbucket.org/acme/widgets.git")
            .expect("Bitbucket HTTPS URL");
        let payload = bitbucket_credential_payload(&context, "test-token-123")
            .expect("valid API token payload");
        let text = String::from_utf8(payload).expect("credential protocol is UTF-8");
        assert!(text.contains("password=test-token-123\n"));
        assert!(text.contains("username=x-bitbucket-api-token-auth\n"));

        let argv = with_bitbucket_username(
            "https://bitbucket.org/acme/widgets.git",
            vec!["push".into(), "--progress".into()],
        );
        assert_eq!(argv[0], "-c");
        assert_eq!(argv[1], "credential.username=x-bitbucket-api-token-auth");
        assert!(!argv.join(" ").contains("test-token-123"));
        assert!(bitbucket_credential_payload(&context, "bad\ntoken").is_err());
    }

    #[tokio::test]
    async fn bitbucket_token_round_trips_through_an_isolated_git_helper() {
        use std::process::Command as StdCommand;

        let root = std::env::temp_dir().join(format!(
            "gitdock-bitbucket-helper-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp repository");
        let helper_file = root.join("credentials");
        let git = |args: &[&str]| {
            let status = StdCommand::new("git")
                .current_dir(&root)
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .status()
                .expect("spawn git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init"]);
        git(&["config", "--local", "--add", "credential.helper", ""]);
        let helper = format!("store --file={}", helper_file.display());
        git(&["config", "--local", "--add", "credential.helper", &helper]);
        git(&["config", "--local", "credential.useHttpPath", "true"]);

        let runner = git_runner().expect("Git runner");
        let context = bitbucket_credential_context("https://bitbucket.org/acme/widgets.git")
            .expect("Bitbucket HTTPS URL");
        let payload = bitbucket_credential_payload(&context, "fixture-token-456")
            .expect("credential payload");
        let approved = runner
            .run_with_stdin(
                &root,
                &["credential", "approve"],
                &payload,
                crate::git::WRITE_TIMEOUT,
            )
            .await
            .expect("approve credential");
        assert!(approved.success);

        let lookup = format!(
            "protocol=https\nhost={}\npath={}\nusername={}\n\n",
            context.host, context.path, context.username
        );
        let filled = runner
            .run_with_stdin(
                &root,
                &["credential", "fill"],
                lookup.as_bytes(),
                crate::git::WRITE_TIMEOUT,
            )
            .await
            .expect("fill credential");
        assert!(filled.success);
        let output = String::from_utf8(filled.stdout).expect("credential output");
        assert!(output.contains("password=fixture-token-456\n"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn execute_remote_reports_success() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let outcome =
            execute_remote("/bin/true", &[], std::path::Path::new("/tmp"), preset(), tx).await;
        assert_eq!(outcome, JobOutcome::Ok);
    }

    #[tokio::test]
    async fn execute_remote_reports_failure() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let outcome = execute_remote(
            "/bin/false",
            &[],
            std::path::Path::new("/tmp"),
            preset(),
            tx,
        )
        .await;
        assert!(matches!(outcome, JobOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn execute_remote_preserves_safe_auth_recovery() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let outcome = execute_remote(
            "/bin/sh",
            &[
                "-c".to_string(),
                "echo \"fatal: could not read Username for 'https://secret.example/repo': terminal prompts disabled\" >&2; exit 128".to_string(),
            ],
            std::path::Path::new("/tmp"),
            preset(),
            tx,
        )
        .await;
        let JobOutcome::Failed(error) = outcome else {
            panic!("expected auth failure");
        };
        assert_eq!(error.code, ErrorCode::AUTH_REQUIRED);
        assert_eq!(error.recovery, RecoveryAction::Authenticate);
        assert!(!error.message.contains("secret.example"));
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn execute_remote_honors_pre_set_cancel() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        let outcome = execute_remote(
            "/bin/sleep",
            &["30".to_string()],
            std::path::Path::new("/tmp"),
            cancel,
            tx,
        )
        .await;
        assert_eq!(outcome, JobOutcome::Cancelled);
    }

    #[tokio::test]
    async fn execute_remote_round_trips_push_and_fetch() {
        use std::process::Command as StdCommand;

        let root =
            std::env::temp_dir().join(format!("gitdock-t11-{}-roundtrip", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp root");
        let git = |cwd: &std::path::Path, args: &[&str]| {
            let status = StdCommand::new("git")
                .current_dir(cwd)
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .expect("spawn git");
            assert!(status.success(), "git {args:?} failed");
        };
        let rev = |cwd: &std::path::Path, rev: &str| {
            String::from_utf8(
                StdCommand::new("git")
                    .current_dir(cwd)
                    .args(["rev-parse", rev])
                    .env("GIT_TERMINAL_PROMPT", "0")
                    .output()
                    .expect("rev-parse")
                    .stdout,
            )
            .expect("utf8")
            .trim()
            .to_string()
        };

        git(&root, &["init", "--bare", "origin.git"]);
        git(&root, &["clone", "origin.git", "a"]);
        let a = root.join("a");
        git(&a, &["checkout", "-b", "main"]);
        std::fs::write(a.join("file.txt"), "v1\n").expect("write");
        git(&a, &["add", "file.txt"]);
        git(
            &a,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@x",
                "commit",
                "-m",
                "v1",
            ],
        );

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let push = execute_remote(
            "git",
            &[
                "push".to_string(),
                "--progress".to_string(),
                "origin".to_string(),
                "HEAD:main".to_string(),
            ],
            &a,
            preset(),
            tx,
        )
        .await;
        assert_eq!(push, JobOutcome::Ok);
        let pushed = rev(&a, "HEAD");
        assert_eq!(rev(&root.join("origin.git"), "main"), pushed);

        git(&root, &["clone", "origin.git", "b"]);
        let b = root.join("b");
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let fetch = execute_remote(
            "git",
            &[
                "fetch".to_string(),
                "--progress".to_string(),
                "origin".to_string(),
            ],
            &b,
            preset(),
            tx,
        )
        .await;
        assert_eq!(fetch, JobOutcome::Ok);
        assert_eq!(rev(&b, "origin/main"), pushed);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn execute_remote_forwards_progress_percentages() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let outcome = execute_remote(
            "/bin/sh",
            &[
                "-c".to_string(),
                "echo 'Receiving objects:  42%' >&2".to_string(),
            ],
            std::path::Path::new("/tmp"),
            preset(),
            tx,
        )
        .await;
        assert_eq!(outcome, JobOutcome::Ok);
        assert_eq!(rx.recv().await, Some((42, "receiving objects".to_string())));
    }

    #[test]
    fn pr_detects_github_bitbucket_and_gitlab_https_and_ssh() {
        let github = detect_pr_provider("https://github.com/acme/widgets.git").expect("github");
        assert_eq!(github.provider, PrProvider::GitHub);
        assert_eq!(github.api_base, "https://api.github.com");
        assert_eq!(github.namespace, "acme");
        assert_eq!(github.repo, "widgets");

        let ssh = detect_pr_provider("git@github.com:acme/widgets.git").expect("github ssh");
        assert_eq!(ssh.provider, PrProvider::GitHub);

        let bitbucket =
            detect_pr_provider("https://bitbucket.org/acme/widgets").expect("bitbucket");
        assert_eq!(bitbucket.provider, PrProvider::Bitbucket);

        let gitlab = detect_pr_provider("https://gitlab.com/group/sub/widgets.git")
            .expect("gitlab subgroup");
        assert_eq!(gitlab.provider, PrProvider::GitLab);
        assert_eq!(gitlab.namespace, "group/sub");
        assert_eq!(gitlab.repo, "widgets");

        let bitbucket_user = detect_pr_provider("https://acme@bitbucket.org/acme/widgets.git")
            .expect("bitbucket userinfo");
        assert_eq!(bitbucket_user.provider, PrProvider::Bitbucket);
        assert_eq!(bitbucket_user.namespace, "acme");
        assert_eq!(bitbucket_user.repo, "widgets");
        let github_user = detect_pr_provider("https://user:token@github.com/acme/widgets.git")
            .expect("github userinfo");
        assert_eq!(github_user.provider, PrProvider::GitHub);

        assert!(detect_pr_provider("https://example.com/acme/widgets.git").is_none());
        assert!(detect_pr_provider("git@github.com:lonely.git").is_none());
        assert!(detect_pr_provider("https://github.com/acme/widgets.git\n").is_some());
        assert!(detect_pr_provider("https://github.com/acme/widgets.git\u{7}").is_none());
        assert_eq!(
            pr_endpoint(&github),
            "https://api.github.com/repos/acme/widgets/pulls"
        );
        assert_eq!(
            pr_endpoint(&gitlab),
            "https://gitlab.com/api/v4/projects/group%2Fsub%2Fwidgets/merge_requests"
        );
    }

    #[test]
    fn pr_payloads_match_each_provider_schema() {
        let github = pr_request_json(PrProvider::GitHub, "T", "D", "feat", "main");
        assert_eq!(github["head"], serde_json::json!("feat"));
        assert_eq!(github["base"], serde_json::json!("main"));

        let bitbucket = pr_request_json(PrProvider::Bitbucket, "T", "D", "feat", "main");
        assert_eq!(
            bitbucket.pointer("/source/branch/name"),
            Some(&serde_json::json!("feat"))
        );
        assert_eq!(
            bitbucket.pointer("/destination/branch/name"),
            Some(&serde_json::json!("main"))
        );

        let gitlab = pr_request_json(PrProvider::GitLab, "T", "D", "feat", "main");
        assert_eq!(gitlab["source_branch"], serde_json::json!("feat"));
        assert_eq!(gitlab["target_branch"], serde_json::json!("main"));
    }

    #[test]
    fn pr_parses_each_provider_response_and_rejects_junk() {
        let (url, reference) = pr_parse_response(
            PrProvider::GitHub,
            br#"{"html_url":"https://github.com/acme/widgets/pull/7","number":7}"#,
        )
        .expect("github response");
        assert_eq!(url, "https://github.com/acme/widgets/pull/7");
        assert_eq!(reference, "#7");

        let (url, reference) = pr_parse_response(
            PrProvider::Bitbucket,
            br#"{"id":9,"links":{"html":{"href":"https://bitbucket.org/acme/widgets/pull-requests/9"}}}"#,
        )
        .expect("bitbucket response");
        assert_eq!(url, "https://bitbucket.org/acme/widgets/pull-requests/9");
        assert_eq!(reference, "#9");

        let (url, reference) = pr_parse_response(
            PrProvider::GitLab,
            br#"{"iid":3,"web_url":"https://gitlab.com/group/widgets/-/merge_requests/3"}"#,
        )
        .expect("gitlab response");
        assert_eq!(url, "https://gitlab.com/group/widgets/-/merge_requests/3");
        assert_eq!(reference, "!3");

        assert!(pr_parse_response(PrProvider::GitHub, b"not json").is_err());
        assert!(pr_parse_response(PrProvider::GitHub, br#"{"number":1}"#).is_err());
        assert!(pr_parse_response(
            PrProvider::GitHub,
            br#"{"html_url":"http://evil.local/x","number":1}"#,
        )
        .is_err());
    }

    #[tokio::test]
    async fn pr_credential_fill_reads_an_isolated_helper_entry() {
        use std::process::Command as StdCommand;

        let root = std::env::temp_dir().join(format!(
            "gitdock-pr-credential-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("temp repository");
        let helper_file = root.join("credentials");
        let git = |args: &[&str]| {
            let status = StdCommand::new("git")
                .current_dir(&root)
                .args(args)
                .env("GIT_TERMINAL_PROMPT", "0")
                .status()
                .expect("spawn git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init"]);
        git(&["config", "--local", "--add", "credential.helper", ""]);
        let helper = format!("store --file={}", helper_file.display());
        git(&["config", "--local", "--add", "credential.helper", &helper]);

        let runner = git_runner().expect("Git runner");
        let approved = runner
            .run_with_stdin(
                &root,
                &["credential", "approve"],
                b"protocol=https\nhost=github.com\nusername=octo\npassword=fixture-pat-123\n\n",
                crate::git::WRITE_TIMEOUT,
            )
            .await
            .expect("approve credential");
        assert!(approved.success);

        let (username, password) = pr_credential(&runner, &root, "github.com")
            .await
            .expect("fill credential");
        assert_eq!(username, "octo");
        assert_eq!(password, "fixture-pat-123");
        assert!(pr_credential(&runner, &root, "unknown.example")
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// End-to-end POST against a loopback stub: asserts method, path, auth
    /// header and payload, then serves a canned provider response.
    #[tokio::test]
    async fn pr_create_posts_github_shape_to_a_stub_server() {
        use std::io::{Read, Write};
        use std::sync::{Arc, Mutex};

        let seen = Arc::new(Mutex::new(Vec::<u8>::new()));
        let seen_server = Arc::clone(&seen);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback stub listener");
        let port = listener.local_addr().expect("stub port").port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("stub accept");
            let mut buffer = vec![0u8; 8192];
            let read = stream.read(&mut buffer).expect("stub read");
            seen_server
                .lock()
                .expect("stub lock")
                .extend_from_slice(&buffer[..read]);
            let body = r#"{"html_url":"https://github.com/acme/widgets/pull/7","number":7}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("stub write");
        });

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("stub http client");
        let coords = PrCoords {
            provider: PrProvider::GitHub,
            api_base: format!("http://127.0.0.1:{port}"),
            namespace: "acme".to_string(),
            repo: "widgets".to_string(),
        };
        let endpoint = pr_endpoint(&coords);
        assert_eq!(
            endpoint,
            format!("http://127.0.0.1:{port}/repos/acme/widgets/pulls")
        );
        let response = http
            .post(&endpoint)
            .header("User-Agent", "Octopus/0.1")
            .header("Accept", "application/json")
            .bearer_auth("fixture-pat-123")
            .json(&pr_request_json(
                PrProvider::GitHub,
                "T",
                "D",
                "feat",
                "main",
            ))
            .send()
            .await
            .expect("stub post");
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
        let body = response.bytes().await.expect("stub body");
        let (url, reference) = pr_parse_response(PrProvider::GitHub, &body).expect("stub parse");
        assert_eq!(url, "https://github.com/acme/widgets/pull/7");
        assert_eq!(reference, "#7");
        server.join().expect("stub server");
        let raw = String::from_utf8(seen.lock().expect("stub lock").clone()).expect("stub text");
        assert!(raw.starts_with("POST /repos/acme/widgets/pulls HTTP/1.1"));
        assert!(raw.contains("authorization: Bearer fixture-pat-123"));
        assert!(!raw.to_lowercase().contains("password"));
        assert!(raw.contains(r#""head":"feat""#));
    }

    #[test]
    fn pull_request_create_request_accepts_frontend_camel_case() {
        let request: PullRequestCreateRequest = serde_json::from_value(serde_json::json!({
            "requestId": "req-1",
            "repoId": "repo-1",
            "expectedVersion": 3,
            "remote": null,
            "sourceBranch": "feat/ui",
            "targetBranch": "main",
            "title": "T",
            "description": "D"
        }))
        .expect("frontend camelCase payload deserializes");
        assert_eq!(request.source_branch, "feat/ui");
        assert_eq!(request.target_branch, "main");
        assert_eq!(request.remote, None);
    }
}
