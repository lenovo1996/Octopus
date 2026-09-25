//! Repository discovery.
//!
//! Canonicalizes the selected path, then resolves worktree root, git dir and
//! common dir with `rev-parse`. Supports linked worktrees (`.git` is a file)
//! and subfolder selection. Bare repositories resolve but are rejected at
//! open time with `BARE_REPOSITORY`.

use std::path::{Path, PathBuf};

use crate::domain::{AppError, ErrorCode, RecoveryAction};

use super::runner::{GitRunner, READ_TIMEOUT};

#[derive(Debug, Clone)]
pub struct DiscoveredRepo {
    pub worktree_root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub bare: bool,
    pub object_format: String,
}

fn one_line(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    text.lines().next().map(|line| line.trim().to_string())
}

fn git_err(message: impl Into<String>, code: ErrorCode) -> AppError {
    AppError::new(code, message, RecoveryAction::ChooseRepository, false)
}

pub async fn discover(runner: &GitRunner, selected: &Path) -> Result<DiscoveredRepo, AppError> {
    let start = std::fs::canonicalize(selected).map_err(|_| {
        git_err(
            "Selected folder is missing or inaccessible",
            ErrorCode::REPO_UNAVAILABLE,
        )
    })?;
    if !start.is_dir() {
        return Err(git_err(
            "Selected path is not a folder",
            ErrorCode::PATH_INVALID,
        ));
    }

    let git_dir_out = runner
        .run(&start, &["rev-parse", "--absolute-git-dir"], READ_TIMEOUT)
        .await
        .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
    if !git_dir_out.success {
        return Err(git_err(
            "Selected folder is not a Git repository",
            ErrorCode::NOT_REPOSITORY,
        ));
    }
    let git_dir = git_dir_out
        .stdout
        .first()
        .and_then(|_| one_line(&git_dir_out.stdout))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| git_err("Git returned an empty git dir", ErrorCode::GIT_ERROR))?;

    let bare_out = runner
        .run(&start, &["rev-parse", "--is-bare-repository"], READ_TIMEOUT)
        .await
        .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
    let bare = one_line(&bare_out.stdout).as_deref() == Some("true");

    let (worktree_root, common_dir) = if bare {
        (git_dir.clone(), git_dir.clone())
    } else {
        let top_out = runner
            .run(&start, &["rev-parse", "--show-toplevel"], READ_TIMEOUT)
            .await
            .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
        if !top_out.success {
            return Err(git_err(
                "Selected folder is not a Git worktree",
                ErrorCode::NOT_REPOSITORY,
            ));
        }
        let root = one_line(&top_out.stdout)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| git_err("Git returned an empty worktree root", ErrorCode::GIT_ERROR))?;
        let common_out = runner
            .run(&start, &["rev-parse", "--git-common-dir"], READ_TIMEOUT)
            .await
            .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
        // --git-common-dir may print a cwd-relative path (e.g. `.git` vs
        // `../.git`); anchor it at the probe cwd before canonicalizing.
        let common = one_line(&common_out.stdout)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| if p.is_relative() { start.join(p) } else { p })
            .unwrap_or_else(|| git_dir.clone());
        (root, common)
    };

    // Canonicalize for stable identity (trust cache + write queue key).
    let worktree_root = std::fs::canonicalize(&worktree_root).unwrap_or(worktree_root);
    let common_dir = std::fs::canonicalize(&common_dir).unwrap_or(common_dir);

    let format_out = runner
        .run(
            &worktree_root,
            &["rev-parse", "--show-object-format"],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
    let object_format = one_line(&format_out.stdout).unwrap_or_else(|| "sha1".to_string());

    Ok(DiscoveredRepo {
        worktree_root,
        git_dir,
        common_dir,
        bare,
        object_format,
    })
}

/// Validate a new branch name with Git itself (plus no option-like names).
pub async fn validate_branch_name(
    runner: &GitRunner,
    cwd: &Path,
    name: &str,
) -> Result<(), AppError> {
    if name.is_empty() || name.starts_with('-') || name.len() > 250 {
        return Err(AppError::new(
            ErrorCode::INVALID_ARGUMENT,
            "Branch name is empty, option-like, or too long",
            RecoveryAction::InspectState,
            false,
        ));
    }
    let out = runner
        .run(cwd, &["check-ref-format", "--branch", name], READ_TIMEOUT)
        .await
        .map_err(|_| git_err("Git probe failed", ErrorCode::GIT_ERROR))?;
    if out.success {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::REF_INVALID,
            format!("Invalid branch name: {name}"),
            RecoveryAction::InspectState,
            false,
        ))
    }
}
