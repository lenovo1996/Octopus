//! `diff_read`: unified viewer source for worktree / index / commit targets.
//!
//! Trust gates mirror the contract: worktree and index diffs need a trusted
//! repo (content filters may run); commit diffs are pure object reads and
//! work read-only. Path identity always comes from listing tokens — the
//! display path in the response never feeds back into Git.

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, DiffDocument, DiffTarget, ErrorCode, RecoveryAction, RequestId, TrustState,
};
use crate::git::{read_commit_diff, read_index_diff, read_worktree_diff, DiffError, GitRunner};
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
        "Trust this repository to read working-copy diffs",
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

fn check_oid(object_format: &str, oid: &str) -> Result<(), AppError> {
    let expected = if object_format == "sha256" { 64 } else { 40 };
    if oid.len() == expected && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(bad_request("Malformed object id"))
    }
}

fn diff_error(error: DiffError) -> AppError {
    match error {
        DiffError::TooLarge => AppError::new(
            ErrorCode::OUTPUT_LIMIT,
            "Diff exceeds the size limit; open the file externally.",
            RecoveryAction::InspectState,
            false,
        ),
        DiffError::Invalid(message) => bad_request(message),
        DiffError::GitFailed(_) => AppError::new(
            ErrorCode::GIT_ERROR,
            "Failed to read this diff.",
            RecoveryAction::Refresh,
            true,
        ),
        DiffError::Run(run) => match run {
            crate::git::runner::RunError::TimedOut => AppError::new(
                ErrorCode::TIMEOUT,
                "Reading the diff timed out.",
                RecoveryAction::RetryRead,
                true,
            ),
            crate::git::runner::RunError::OutputLimit => AppError::new(
                ErrorCode::OUTPUT_LIMIT,
                "Diff output exceeded the safety bound.",
                RecoveryAction::RetryRead,
                true,
            ),
            crate::git::runner::RunError::SpawnFailed(_) => AppError::new(
                ErrorCode::GIT_ERROR,
                "Failed to read this diff.",
                RecoveryAction::Refresh,
                true,
            ),
        },
    }
}

async fn core_diff(
    runner: &GitRunner,
    registry: &RepoRegistry,
    repo_id: &str,
    target: &DiffTarget,
) -> Result<DiffDocument, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    match target {
        DiffTarget::Worktree { path_id } => {
            if session.trust != TrustState::Trusted {
                return Err(trust_required());
            }
            let (raw, _) = registry.status_resolve(repo_id, path_id)?;
            read_worktree_diff(runner, &session.worktree_root, &raw)
                .await
                .map_err(diff_error)
        }
        DiffTarget::Index { path_id } => {
            if session.trust != TrustState::Trusted {
                return Err(trust_required());
            }
            let (raw, _) = registry.status_resolve(repo_id, path_id)?;
            read_index_diff(runner, &session.worktree_root, &raw)
                .await
                .map_err(diff_error)
        }
        DiffTarget::Commit {
            oid,
            parent_index,
            path_id,
        } => {
            check_oid(&session.object_format, oid)?;
            // Parent identity comes from the cached commit read, never from
            // UI-supplied OIDs: the token only validates against the exact
            // (oid, parent) comparison that issued it.
            let metas =
                crate::git::read_metadata(runner, &session, std::slice::from_ref(oid)).await?;
            let meta = metas.into_iter().next().ok_or_else(|| {
                AppError::new(
                    ErrorCode::REF_INVALID,
                    "Unknown commit",
                    RecoveryAction::Refresh,
                    true,
                )
            })?;
            // Contract (docs/04-ipc-contracts.md): parent index defaults
            // to 0 for every non-root commit, merges included.
            let used_parent: Option<usize> = match (*parent_index, meta.parents.len()) {
                (None, 0) => None,
                (None, _) => Some(0),
                (Some(i), n) if i < n => Some(i),
                (Some(_), _) => return Err(bad_request("Parent index is out of range")),
            };
            let parent_oid: Option<String> = used_parent.map(|i| meta.parents[i].clone());
            let (raw, _) = registry.commit_file_resolve(repo_id, oid, used_parent, path_id)?;
            read_commit_diff(
                runner,
                &session.worktree_root,
                &raw,
                oid,
                parent_oid.as_deref(),
            )
            .await
            .map_err(diff_error)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReadRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub target: DiffTarget,
}

#[tauri::command]
pub async fn diff_read(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: DiffReadRequest,
) -> Result<ApiResult<DiffDocument>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let registry = registry.lock().await;
        core_diff(&runner, &registry, &request.repo_id, &request.target).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

pub mod prelude {
    pub use super::{diff_read, DiffReadRequest};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TrustState;
    use std::process::Command as StdCommand;

    #[test]
    fn diff_request_accepts_frontend_json_for_every_source() {
        let targets = [
            (
                serde_json::json!({"kind": "worktree", "pathId": "status-token"}),
                DiffTarget::Worktree {
                    path_id: "status-token".into(),
                },
            ),
            (
                serde_json::json!({"kind": "index", "pathId": "status-token"}),
                DiffTarget::Index {
                    path_id: "status-token".into(),
                },
            ),
            (
                serde_json::json!({"kind": "commit", "oid": "commit-oid", "parentIndex": 1, "pathId": "commit-token"}),
                DiffTarget::Commit {
                    oid: "commit-oid".into(),
                    parent_index: Some(1),
                    path_id: "commit-token".into(),
                },
            ),
            (
                serde_json::json!({"kind": "commit", "oid": "root-oid", "parentIndex": null, "pathId": "root-token"}),
                DiffTarget::Commit {
                    oid: "root-oid".into(),
                    parent_index: None,
                    path_id: "root-token".into(),
                },
            ),
        ];
        for (target, expected) in targets {
            let request: DiffReadRequest = serde_json::from_value(serde_json::json!({
                "requestId": "diff-contract", "repoId": "repo", "target": target
            }))
            .expect("frontend diff payload must deserialize at the command boundary");
            assert_eq!(request.target, expected);
        }
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

    fn temp_repo(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("gitdock-t08-cmd-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        (dir, repo)
    }

    #[tokio::test]
    async fn worktree_diff_is_trust_gated_and_token_bound() {
        let (_dir, repo) = temp_repo("gated");
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        git(&repo, &["commit", "-m", "a"]);
        std::fs::write(repo.join("a.txt"), "a\nb\n").expect("write");

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::ReadOnly);

        // Read-only sessions cannot read worktree diffs (content filters).
        let denied = core_diff(
            &runner,
            &registry,
            &repo_id,
            &DiffTarget::Worktree {
                path_id: "bogus".to_string(),
            },
        )
        .await;
        assert!(matches!(
            denied,
            Err(ref e) if e.code == ErrorCode::TRUST_REQUIRED
        ));

        registry.get_mut(&repo_id).expect("session").trust = TrustState::Trusted;
        // A token from another listing generation is stale, never guessed.
        let stale = core_diff(
            &runner,
            &registry,
            &repo_id,
            &DiffTarget::Worktree {
                path_id: format!("{repo_id}:999:0"),
            },
        )
        .await;
        assert!(matches!(
            stale,
            Err(ref e) if e.code == ErrorCode::STALE_STATE
        ));
    }

    #[tokio::test]
    async fn frontend_json_reads_index_and_worktree_with_distinct_content() {
        let (_dir, repo) = temp_repo("json-index-worktree");
        std::fs::write(repo.join("file.txt"), "base\n").expect("write");
        git(&repo, &["add", "file.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        std::fs::write(repo.join("file.txt"), "base\nstaged\n").expect("write");
        git(&repo, &["add", "file.txt"]);
        std::fs::write(repo.join("file.txt"), "base\nstaged\nunstaged\n").expect("write");
        let runner = git_runner().expect("git");
        let mut registry = RepoRegistry::default();
        let repo_id = open_repo(&mut registry, &repo, TrustState::Trusted);
        registry.status_put(&repo_id, vec![b"file.txt".to_vec()], vec![None]);
        let path_id = registry.status_path_id(&repo_id, 0).expect("path token");
        for (kind, expected) in [("index", "staged"), ("worktree", "unstaged")] {
            let request: DiffReadRequest = serde_json::from_value(serde_json::json!({
                "requestId": "diff-json", "repoId": repo_id,
                "target": { "kind": kind, "pathId": path_id }
            }))
            .expect("frontend JSON");
            let doc = core_diff(&runner, &registry, &request.repo_id, &request.target)
                .await
                .expect("diff from native request");
            let added: Vec<_> = doc
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|line| line.kind == "add")
                .map(|line| line.text.as_str())
                .collect();
            assert_eq!(added, vec![expected]);
        }
    }

    #[tokio::test]
    async fn commit_diff_works_read_only_via_details_tokens() {
        let (_dir, repo) = temp_repo("commit");
        std::fs::write(repo.join("f.txt"), "one\n").expect("write");
        git(&repo, &["add", "f.txt"]);
        git(&repo, &["commit", "-m", "f"]);

        let runner = git_runner().expect("system git");
        let mut registry = RepoRegistry::default();
        // Read-only on purpose: commit diffs are pure object reads.
        let repo_id = open_repo(&mut registry, &repo, TrustState::ReadOnly);
        let oid = {
            let out = StdCommand::new("git")
                .current_dir(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        // Issue tokens through the details path, exactly like the UI does.
        let details =
            crate::commands::history::core_details(&runner, &mut registry, &repo_id, &oid, None)
                .await
                .expect("details");
        assert_eq!(details.files.len(), 1);
        let token = details.files[0].path_id.clone();
        assert!(!token.is_empty());

        let request: DiffReadRequest = serde_json::from_value(serde_json::json!({
            "requestId": "commit-diff-json", "repoId": repo_id,
            "target": { "kind": "commit", "oid": oid, "parentIndex": null, "pathId": token }
        }))
        .expect("native request JSON");
        let doc = core_diff(&runner, &registry, &request.repo_id, &request.target)
            .await
            .expect("commit diff");
        assert_eq!(doc.kind, "text");
        assert_eq!(doc.additions, Some(1));

        // A worktree token never validates as a commit token.
        let cross = core_diff(
            &runner,
            &registry,
            &repo_id,
            &DiffTarget::Commit {
                oid,
                parent_index: None,
                path_id: format!("{repo_id}:1:0"),
            },
        )
        .await;
        assert!(matches!(
            cross,
            Err(ref e) if e.code == ErrorCode::STALE_STATE
        ));
    }
}
