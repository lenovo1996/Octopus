//! Commit plumbing: identity reads, preconditions, stdin messages.
//!
//! Messages travel via stdin (`commit -F -`), never argv or an editor.
//! Identity values come from `git config --show-origin`; only the config
//! source scope is reported, never secrets.

use std::path::Path;

use crate::git::runner::{GitRunner, RunError, READ_TIMEOUT, WRITE_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: Option<String>,
    pub email: Option<String>,
    /// "local" | "global" | "system" | "command" | "multiple" | "missing".
    pub scope: String,
    pub signing: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CommitError {
    Run(RunError),
    GitFailed(String),
}

impl From<RunError> for CommitError {
    fn from(value: RunError) -> Self {
        CommitError::Run(value)
    }
}

async fn config_get(runner: &GitRunner, cwd: &Path, key: &str) -> Option<(String, String)> {
    let out = runner
        .run(
            cwd,
            &["config", "--show-origin", "--get", key],
            READ_TIMEOUT,
        )
        .await
        .ok()?;
    if !out.success {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let (origin, value) = line.trim().split_once('\t')?;
    Some((origin.trim().to_string(), value.to_string()))
}

fn scope_of(origin: &str) -> &str {
    let path = origin.strip_prefix("file:").unwrap_or(origin);
    if path.contains(".git/config") {
        "local"
    } else if path.ends_with(".gitconfig") || path.contains("/git/config") {
        "global"
    } else if path.starts_with("/etc/") {
        "system"
    } else if origin == "command line" {
        "command"
    } else {
        "multiple"
    }
}

/// Effective author identity with its config scope. Name and email may come
/// from different scopes; then scope reads "multiple".
pub async fn read_identity(runner: &GitRunner, cwd: &Path) -> Result<Identity, CommitError> {
    let name = config_get(runner, cwd, "user.name").await;
    let email = config_get(runner, cwd, "user.email").await;
    let signing = runner
        .run(cwd, &["config", "--get", "commit.gpgsign"], READ_TIMEOUT)
        .await
        .map(|out| out.success && String::from_utf8_lossy(&out.stdout).trim() == "true")
        .unwrap_or(false);
    let scope = match (&name, &email) {
        (None, None) => "missing".to_string(),
        (Some((n_origin, _)), Some((e_origin, _))) => {
            let (n, e) = (scope_of(n_origin), scope_of(e_origin));
            if n == e {
                n.to_string()
            } else {
                "multiple".to_string()
            }
        }
        (Some((origin, _)), None) | (None, Some((origin, _))) => scope_of(origin).to_string(),
    };
    Ok(Identity {
        name: name.map(|(_, v)| v),
        email: email.map(|(_, v)| v),
        scope,
        signing,
    })
}

/// True while the index holds unmerged entries (stages 1–3 present).
pub async fn has_unmerged(runner: &GitRunner, cwd: &Path) -> Result<bool, CommitError> {
    let out = runner
        .run(cwd, &["ls-files", "-u", "-z"], READ_TIMEOUT)
        .await?;
    if !out.success {
        let mut message = String::from_utf8_lossy(&out.stderr).into_owned();
        message.truncate(300);
        return Err(CommitError::GitFailed(message));
    }
    Ok(!out.stdout.iter().all(|b| *b == 0))
}

/// True when the index holds no staged changes vs HEAD (or vs the empty
/// tree when HEAD is unborn — plain `--cached` already handles that).
pub async fn index_is_empty(runner: &GitRunner, cwd: &Path) -> Result<bool, CommitError> {
    let out = runner
        .run(cwd, &["diff", "--cached", "--quiet"], READ_TIMEOUT)
        .await?;
    Ok(out.success)
}

/// Commit exactly the index with a stdin message. Returns the new HEAD oid.
/// Hooks and signing run as configured; their failures surface as
/// `GitFailed` with truncated stderr for classification upstream.
pub async fn commit_staged(
    runner: &GitRunner,
    cwd: &Path,
    message: &[u8],
) -> Result<String, CommitError> {
    let out = runner
        .run_with_stdin(cwd, &["commit", "-F", "-"], message, WRITE_TIMEOUT)
        .await?;
    if !out.success {
        let mut message = String::from_utf8_lossy(&out.stderr).into_owned();
        message.truncate(2000);
        return Err(CommitError::GitFailed(message));
    }
    let oid_out = runner
        .run(cwd, &["rev-parse", "HEAD"], READ_TIMEOUT)
        .await?;
    if !oid_out.success {
        return Err(CommitError::GitFailed(
            "Commit ran but HEAD is unreadable".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&oid_out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn git(cwd: &Path, args: &[&str]) {
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
        let dir = std::env::temp_dir().join(format!("gitdock-t10-{label}"));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&dir, &["init", "-b", "main", "repo"]);
        (dir, repo)
    }

    fn runner() -> GitRunner {
        GitRunner::new(GitRunner::resolve_from_path().expect("system git"))
    }

    #[tokio::test]
    async fn identity_reports_scope_without_secrets() {
        let (_dir, repo) = temp_repo("identity");
        let runner = runner();
        // Repo-local identity wins over the (absent) global one.
        git(&repo, &["config", "user.name", "Local Dev"]);
        git(&repo, &["config", "user.email", "local@example.com"]);
        let identity = read_identity(&runner, &repo).await.expect("identity");
        assert_eq!(identity.name.as_deref(), Some("Local Dev"));
        assert_eq!(identity.email.as_deref(), Some("local@example.com"));
        assert_eq!(identity.scope, "local");
        // Signing flag only, never key material.
        assert!(!identity.signing);
    }

    #[tokio::test]
    async fn empty_index_and_unmerged_detection() {
        let (_dir, repo) = temp_repo("preconds");
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        let runner = runner();
        assert!(index_is_empty(&runner, &repo).await.expect("empty"));
        assert!(!has_unmerged(&runner, &repo).await.expect("merged"));
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        assert!(!index_is_empty(&runner, &repo).await.expect("staged"));
    }
}
