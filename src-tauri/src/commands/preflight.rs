//! `app_preflight`: real environment probe (Git path/version, platform).
//! No repository access, no network, no config writes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command as TokioCommand;

use crate::domain::{ApiResult, AppError, ErrorCode, RecoveryAction, RequestId};

/// Minimum supported system Git (docs/10-build-release.md).
const MIN_GIT_MAJOR: u64 = 2;
const MIN_GIT_MINOR: u64 = 43;

/// Env vars that could redirect Git to another repository; never inherited.
const SANITIZED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPreflightRequest {
    pub request_id: RequestId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightFeatures {
    pub local_operations: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightData {
    /// Resolved executable path used for the probe. Display only.
    pub git_path: String,
    /// Raw version string reported by Git, e.g. "2.43.0".
    pub git_version: String,
    /// True when the version meets the supported baseline.
    pub git_supported: bool,
    pub platform: String,
    pub arch: String,
    pub features: PreflightFeatures,
}

/// Parse `git --version` stdout (`git version 2.43.0` + optional suffixes).
pub fn parse_git_version(stdout: &str) -> Option<(String, u64, u64)> {
    let first_line = stdout.lines().next()?.trim();
    let version_token = first_line
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut parts = version_token.split('.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next().unwrap_or("0").parse().ok()?;
    Some((version_token.to_string(), major, minor))
}

pub fn version_supported(major: u64, minor: u64) -> bool {
    major > MIN_GIT_MAJOR || (major == MIN_GIT_MAJOR && minor >= MIN_GIT_MINOR)
}

/// Resolve `git` against PATH without spawning a shell.
fn resolve_git_path() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join("git");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[tauri::command]
pub async fn app_preflight(request: AppPreflightRequest) -> ApiResult<PreflightData> {
    let request_id = request.request_id;
    if request_id.is_empty() || request_id.len() > 128 {
        return ApiResult::err(
            AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "requestId must be 1..=128 characters",
                RecoveryAction::RetryRead,
                false,
            ),
            request_id,
        );
    }

    let git_path = match resolve_git_path() {
        Some(path) => path,
        None => {
            return ApiResult::err(
                AppError::new(
                    ErrorCode::GIT_NOT_FOUND,
                    "Git executable not found on PATH",
                    RecoveryAction::ConfigureGit,
                    false,
                ),
                request_id,
            )
        }
    };

    let mut command = TokioCommand::new(&git_path);
    // Global `-c` flags must precede the subcommand/flag they configure.
    command
        .arg("-c")
        .arg("color.ui=false")
        .arg("--version")
        .env("GIT_TERMINAL_PROMPT", "0");
    for key in SANITIZED_GIT_ENV {
        command.env_remove(key);
    }

    let output = match tokio::time::timeout(Duration::from_secs(30), command.output()).await {
        Err(_) => {
            return ApiResult::err(
                AppError::new(
                    ErrorCode::TIMEOUT,
                    "Git version probe timed out",
                    RecoveryAction::RetryRead,
                    true,
                ),
                request_id,
            )
        }
        Ok(Err(e)) => {
            return ApiResult::err(
                AppError::new(
                    ErrorCode::IO_ERROR,
                    format!("Failed to run Git probe: {e}"),
                    RecoveryAction::ConfigureGit,
                    false,
                ),
                request_id,
            )
        }
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        return ApiResult::err(
            AppError::new(
                ErrorCode::GIT_ERROR,
                "Git version probe failed",
                RecoveryAction::ConfigureGit,
                false,
            ),
            request_id,
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some((version, major, minor)) = parse_git_version(&stdout) else {
        return ApiResult::err(
            AppError::new(
                ErrorCode::GIT_VERSION_UNSUPPORTED,
                "Unrecognized Git version output",
                RecoveryAction::ConfigureGit,
                false,
            ),
            request_id,
        );
    };

    if !version_supported(major, minor) {
        return ApiResult::err(
            AppError::new(
                ErrorCode::GIT_VERSION_UNSUPPORTED,
                format!("Git {version} is below the supported baseline 2.43"),
                RecoveryAction::ConfigureGit,
                false,
            ),
            request_id,
        );
    }

    ApiResult::ok(
        PreflightData {
            git_path: git_path.to_string_lossy().into_owned(),
            git_version: version,
            git_supported: true,
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            features: PreflightFeatures {
                local_operations: true,
            },
        },
        request_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_suffixed_versions() {
        assert_eq!(
            parse_git_version("git version 2.43.0\n"),
            Some(("2.43.0".to_string(), 2, 43))
        );
        assert_eq!(
            parse_git_version("git version 2.43.0.windows.1\n"),
            Some(("2.43.0.windows.1".to_string(), 2, 43))
        );
        assert_eq!(
            parse_git_version("git version 3.1\n"),
            Some(("3.1".to_string(), 3, 1))
        );
    }

    #[test]
    fn rejects_non_git_output() {
        assert_eq!(parse_git_version(""), None);
        assert_eq!(parse_git_version("version 2.43.0\n"), None);
        assert_eq!(parse_git_version("git version x.y\n"), None);
    }

    #[test]
    fn enforces_minimum_baseline() {
        assert!(version_supported(2, 43));
        assert!(version_supported(2, 44));
        assert!(version_supported(3, 0));
        assert!(!version_supported(2, 42));
        assert!(!version_supported(1, 99));
    }

    /// Smoke against the real system Git. Touches no repository.
    #[tokio::test]
    async fn preflight_reports_real_git() {
        let request = AppPreflightRequest {
            request_id: "req-smoke".to_string(),
        };
        match app_preflight(request).await {
            ApiResult::Ok { data, .. } => {
                assert!(data.git_supported, "system Git below baseline");
                assert!(!data.git_version.is_empty());
                assert!(!data.git_path.is_empty());
            }
            ApiResult::Err { error, .. } => {
                panic!("preflight failed on system Git: {:?}", error);
            }
        }
    }

    #[tokio::test]
    async fn preflight_rejects_empty_request_id() {
        let request = AppPreflightRequest {
            request_id: String::new(),
        };
        match app_preflight(request).await {
            ApiResult::Err { error, .. } => {
                assert_eq!(error.code, ErrorCode::INVALID_ARGUMENT);
            }
            ApiResult::Ok { .. } => panic!("expected INVALID_ARGUMENT"),
        }
    }
}
