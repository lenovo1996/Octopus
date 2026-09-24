//! Bounded system-Git subprocess runner (docs/05-git-engine.md §1).
//!
//! Fixed executable + argv list, never a shell. Repository-redirecting env
//! vars are stripped; interactive prompts and pagers are disabled.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command as TokioCommand;

/// Env vars that could redirect Git to another repository; never inherited.
pub const SANITIZED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
];

/// Default timeouts: reads 30s, local writes 120s, network 300s.
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(120);
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);

/// Stdout safety bound; larger output must stream/trim, not accumulate.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    SpawnFailed(String),
    TimedOut,
    OutputLimit,
}

pub struct GitRunner {
    exe: PathBuf,
}

impl GitRunner {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe }
    }

    /// Resolve `git` against PATH without spawning a shell.
    pub fn resolve_from_path() -> Option<PathBuf> {
        let paths = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join("git");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// Run Git with fixed argv in `cwd`. No shell, no prompt, no pager.
    /// Argv items are OS strings so non-UTF-8 paths pass through byte-exact.
    pub async fn run<S: AsRef<OsStr>>(
        &self,
        cwd: &Path,
        argv: &[S],
        timeout: Duration,
    ) -> Result<GitOutput, RunError> {
        self.run_with_env(cwd, argv, timeout, &[]).await
    }

    /// Same as [`run`](Self::run) but feeds `stdin_bytes` to the child.
    /// Used for payloads that must never travel as argv (commit messages
    /// via `-F -`). Size is bounded like any other output.
    pub async fn run_with_stdin<S: AsRef<OsStr>>(
        &self,
        cwd: &Path,
        argv: &[S],
        stdin_bytes: &[u8],
        timeout: Duration,
    ) -> Result<GitOutput, RunError> {
        use tokio::io::AsyncWriteExt;
        if stdin_bytes.len() > MAX_OUTPUT_BYTES {
            return Err(RunError::OutputLimit);
        }
        let mut command = TokioCommand::new(&self.exe);
        command
            .current_dir(cwd)
            .args(argv)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for key in SANITIZED_GIT_ENV {
            command.env_remove(key);
        }
        let mut child = command
            .spawn()
            .map_err(|e| RunError::SpawnFailed(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            tokio::time::timeout(timeout, stdin.write_all(stdin_bytes))
                .await
                .map_err(|_| RunError::TimedOut)?
                .map_err(|e| RunError::SpawnFailed(e.to_string()))?;
        }
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| RunError::TimedOut)?
            .map_err(|e| RunError::SpawnFailed(e.to_string()))?;
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err(RunError::OutputLimit);
        }
        Ok(GitOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.status.success(),
        })
    }

    /// Same as [`run`](Self::run) plus caller-chosen extra env vars.
    /// Sanitized redirect vars are still stripped, so this cannot be used
    /// to point Git at another repository. Used for behavior flags such as
    /// `GIT_OPTIONAL_LOCKS=0` on read-only worktree status.
    pub async fn run_with_env<S: AsRef<OsStr>>(
        &self,
        cwd: &Path,
        argv: &[S],
        timeout: Duration,
        extra_env: &[(&str, &str)],
    ) -> Result<GitOutput, RunError> {
        let mut command = TokioCommand::new(&self.exe);
        command
            .current_dir(cwd)
            .args(argv)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_PAGER", "cat")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        for (key, value) in extra_env {
            command.env(key, value);
        }
        for key in SANITIZED_GIT_ENV {
            command.env_remove(key);
        }
        let output = tokio::time::timeout(timeout, command.output())
            .await
            .map_err(|_| RunError::TimedOut)?
            .map_err(|e| RunError::SpawnFailed(e.to_string()))?;
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err(RunError::OutputLimit);
        }
        Ok(GitOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            success: output.status.success(),
        })
    }
}
