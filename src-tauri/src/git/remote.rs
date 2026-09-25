//! Remote sync plumbing: URL policy, upstream resolution, ahead/behind,
//! network error classification and progress parsing (T11).
//!
//! URL policy: HTTPS, SSH, SCP-like SSH and
//! local folders pass. `ext::`, unknown schemes, inline passwords and
//! custom transport helpers are rejected. Logs and UI only ever see the
//! redacted form (no userinfo password, no query/fragment).

use std::path::Path;

use crate::git::runner::{GitRunner, READ_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteKind {
    Local,
    Https,
    Ssh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlError {
    /// User-safe message, already free of secrets.
    Rejected(String),
}

/// Classify a remote URL without contacting the network. Returns the
/// transport kind or a user-safe rejection reason.
pub fn validate_remote_url(url: &str) -> Result<RemoteKind, UrlError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(UrlError::Rejected("Remote URL is empty".to_string()));
    }
    if url.starts_with("ext::") {
        return Err(UrlError::Rejected(
            "External transport helpers (ext::) are not supported".to_string(),
        ));
    }
    if let Some((scheme, rest)) = url.split_once("://") {
        match scheme.to_lowercase().as_str() {
            "https" => {
                if rest.contains('@') && rest.split('@').next().unwrap_or("").contains(':') {
                    return Err(UrlError::Rejected(
                        "Inline passwords in remote URLs are not allowed; use a credential helper"
                            .to_string(),
                    ));
                }
                Ok(RemoteKind::Https)
            }
            "ssh" => {
                if rest.contains('@') && rest.split('@').next().unwrap_or("").contains(':') {
                    return Err(UrlError::Rejected(
                        "Inline passwords in remote URLs are not allowed; use an SSH agent"
                            .to_string(),
                    ));
                }
                Ok(RemoteKind::Ssh)
            }
            "file" => Ok(RemoteKind::Local),
            _ => Err(UrlError::Rejected(format!(
                "Unsupported URL scheme '{scheme}'; use HTTPS, SSH or a local folder"
            ))),
        }
    } else if is_scp_like(url) {
        Ok(RemoteKind::Ssh)
    } else {
        // No scheme: a local folder (absolute, ~, or relative path).
        Ok(RemoteKind::Local)
    }
}

/// `host:path` / `user@host:path` without `://`. Windows drive paths
/// (`C:/...`, `C:\...`) are local folders, not SCP remotes.
fn is_scp_like(url: &str) -> bool {
    if url.len() >= 2
        && url.as_bytes()[0].is_ascii_alphabetic()
        && url.as_bytes()[1] == b':'
        && (url.as_bytes().get(2) == Some(&b'/') || url.as_bytes().get(2) == Some(&b'\\'))
    {
        return false;
    }
    match url.split_once(':') {
        Some((head, _)) => !head.contains('/') && !head.is_empty(),
        None => false,
    }
}

/// Redact a remote URL for logs and UI: drop any userinfo password and
/// query/fragment parts. Never fails closed — unknown shapes pass through
/// only when they contain no `@` secret marker... actually passwords are
/// exactly what `@` userinfo carries, so strip userinfo passwords always.
/// Username portion of userinfo: up to the first `:` or `@`. Anything past
/// it is a password (or attacker-placed confusion) and never survives.
fn user_of(userinfo: &str) -> &str {
    userinfo.split([':', '@']).next().unwrap_or("")
}

pub fn redact_url(url: &str) -> String {
    // Scheme URLs: strip `user:pass@` to `user@`, drop query/fragment. The
    // host anchors on the LAST `@` so `user@pass@host` cannot smuggle the
    // password into the host position.
    if let Some((scheme, rest)) = url.split_once("://") {
        let (authority, path) = match rest.split_once('/') {
            Some((a, p)) => (a, format!("/{p}")),
            None => (rest, String::new()),
        };
        let authority = match authority.rsplit_once('@') {
            Some((userinfo, host)) => {
                let user = user_of(userinfo);
                if user.is_empty() {
                    host.to_string()
                } else {
                    format!("{user}@{host}")
                }
            }
            None => authority.to_string(),
        };
        let path = path.split(['?', '#']).next().unwrap_or("").to_string();
        return format!("{scheme}://{authority}{path}");
    }
    // SCP-like `user:pass@host:path`: redact to `user@host:path`. The split
    // anchors on the LAST `@` (hosts never contain one); a password only
    // counts when a path colon follows, otherwise the shape is a plain
    // `host:path`.
    if is_scp_like(url) {
        if let Some((before_at, after_at)) = url.rsplit_once('@') {
            if before_at.contains(':') && after_at.contains(':') {
                let user = user_of(before_at);
                if !user.is_empty() {
                    return format!("{user}@{after_at}");
                }
            }
        }
    }
    url.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamRef {
    /// Remote name from `branch.<name>.remote`.
    pub remote: String,
    /// Full remote-tracking ref, e.g. `refs/remotes/origin/main`.
    pub full_name: String,
    /// Redacted remote URL.
    pub url: String,
}

/// Effective upstream of the current branch, or `None` when HEAD is
/// detached/unborn or no upstream is configured. Pure local reads only.
pub async fn resolve_upstream(
    runner: &GitRunner,
    cwd: &Path,
) -> Result<Option<UpstreamRef>, String> {
    let head = runner
        .run(
            cwd,
            &["symbolic-ref", "-q", "--short", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .map_err(|e| format!("HEAD read failed: {e:?}"))?;
    if !head.success {
        return Ok(None);
    }
    let branch = String::from_utf8_lossy(&head.stdout).trim().to_string();
    if branch.is_empty() {
        return Ok(None);
    }
    let key = |suffix: &str| format!("branch.{branch}.{suffix}");
    let value = |key: String| async move {
        runner
            .run(cwd, &["config", "--get", &key], READ_TIMEOUT)
            .await
            .ok()
            .filter(|out| out.success)
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let (remote, merge) = tokio::join!(value(key("remote")), value(key("merge")));
    let (remote, merge) = match (remote, merge) {
        (Some(r), Some(m)) if !r.is_empty() && !m.is_empty() => (r, m),
        _ => return Ok(None),
    };
    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    let full_name = format!("refs/remotes/{remote}/{short}");
    // The tracking ref may not exist yet (never fetched): still report the
    // upstream, ahead/behind resolution handles the missing side.
    let url_out = runner
        .run(
            cwd,
            &["config", "--get", &format!("remote.{remote}.url")],
            READ_TIMEOUT,
        )
        .await
        .map_err(|e| format!("remote url read failed: {e:?}"))?;
    let raw_url = String::from_utf8_lossy(&url_out.stdout).trim().to_string();
    if !url_out.success || raw_url.is_empty() {
        return Ok(None);
    }
    validate_remote_url(&raw_url).map_err(|_| {
        format!("Configured remote URL for '{remote}' is not supported in this version")
    })?;
    Ok(Some(UpstreamRef {
        remote,
        full_name,
        url: redact_url(&raw_url),
    }))
}

/// `(ahead, behind)` of HEAD vs `upstream_ref`, treating a missing side as
/// empty (never fetched / unborn tracking ref). `None` only on hard errors.
pub async fn ahead_behind(
    runner: &GitRunner,
    cwd: &Path,
    upstream_ref: &str,
) -> Result<Option<(u64, u64)>, String> {
    async fn exists(runner: &GitRunner, cwd: &Path, rev: &str) -> bool {
        runner
            .run(
                cwd,
                &["rev-parse", "--verify", "--quiet", rev],
                READ_TIMEOUT,
            )
            .await
            .map(|out| out.success)
            .unwrap_or(false)
    }
    let (head_ok, up_ok) = tokio::join!(
        exists(runner, cwd, "HEAD"),
        exists(runner, cwd, upstream_ref)
    );
    if !head_ok && !up_ok {
        return Ok(Some((0, 0)));
    }
    // A missing side counts as empty: count the existing side alone instead
    // of inventing an empty-tree endpoint (the object may not exist).
    if !head_ok {
        let count = count_rev_list(runner, cwd, upstream_ref).await?;
        return Ok(Some((0, count)));
    }
    if !up_ok {
        let count = count_rev_list(runner, cwd, "HEAD").await?;
        return Ok(Some((count, 0)));
    }
    let out = runner
        .run(
            cwd,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{upstream_ref}"),
            ],
            READ_TIMEOUT,
        )
        .await
        .map_err(|e| format!("ahead/behind failed: {e:?}"))?;
    if !out.success {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    let ahead: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let behind: u64 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    Ok(Some((ahead, behind)))
}

async fn count_rev_list(runner: &GitRunner, cwd: &Path, rev: &str) -> Result<u64, String> {
    let out = runner
        .run(cwd, &["rev-list", "--count", rev], READ_TIMEOUT)
        .await
        .map_err(|e| format!("count failed: {e:?}"))?;
    if !out.success {
        return Ok(0);
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0))
}

/// Classify stderr/exit of a failed network op into a user-facing bucket.
/// Pure function: fast, fully unit-tested, no network needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkFault {
    Offline,
    Auth,
    Other,
}

pub fn classify_network_stderr(stderr: &str) -> NetworkFault {
    let text = stderr.to_lowercase();
    const OFFLINE: &[&str] = &[
        "could not resolve host",
        "could not resolve hostname",
        "network is unreachable",
        "no route to host",
        "connection refused",
        "connection timed out",
        "timed out",
        "operation timed out",
        "unable to connect",
        "failed to connect",
        "temporary failure in name resolution",
        "name or service not known",
    ];
    const AUTH: &[&str] = &[
        "authentication failed",
        "permission denied (publickey",
        "permission denied (keyboard",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "askpass",
        "credential",
        "invalid credentials",
        "repository not found",
    ];
    if OFFLINE.iter().any(|marker| text.contains(marker)) {
        NetworkFault::Offline
    } else if AUTH.iter().any(|marker| text.contains(marker)) {
        NetworkFault::Auth
    } else {
        NetworkFault::Other
    }
}

/// Parse a `--progress` sideband line into a 0..=100 percentage.
/// Handles `Receiving objects: 45% (13/28)`, `Resolving deltas: 100%`, etc.
pub fn parse_progress_line(line: &str) -> Option<u32> {
    let percent = line.split('%').next()?;
    let digits: String = percent
        .rsplit([':', ' ', '\r'])
        .next()?
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    let value: u32 = digits.parse().ok()?;
    if line.contains('%') && value <= 100 {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_policy_accepts_and_rejects() {
        assert_eq!(
            validate_remote_url("https://github.com/acme/app.git"),
            Ok(RemoteKind::Https)
        );
        assert_eq!(
            validate_remote_url("git@github.com:acme/app.git"),
            Ok(RemoteKind::Ssh)
        );
        assert_eq!(
            validate_remote_url("ssh://git@github.com/acme/app.git"),
            Ok(RemoteKind::Ssh)
        );
        assert_eq!(
            validate_remote_url("/srv/git/app.git"),
            Ok(RemoteKind::Local)
        );
        assert_eq!(
            validate_remote_url("ext::ssh -i key %S %s"),
            Err(UrlError::Rejected(
                "External transport helpers (ext::) are not supported".to_string()
            ))
        );
        assert!(validate_remote_url("ftp://host/app.git").is_err());
        assert!(validate_remote_url("https://user:s3cret@host/app.git").is_err());
        assert!(validate_remote_url("https://oauthtoken@host/app.git").is_ok());
    }

    #[test]
    fn redaction_strips_secrets_but_keeps_routing() {
        assert_eq!(
            redact_url("https://user:s3cret@github.com/acme/app.git"),
            "https://user@github.com/acme/app.git"
        );
        assert_eq!(
            redact_url("https://github.com/acme/app.git?token=abc#frag"),
            "https://github.com/acme/app.git"
        );
        assert_eq!(
            redact_url("deploy:k3y@github.com:acme/app.git"),
            "deploy@github.com:acme/app.git"
        );
        assert_eq!(redact_url("/srv/git/app.git"), "/srv/git/app.git");
        assert_eq!(
            redact_url("git@github.com:acme/app.git"),
            "git@github.com:acme/app.git"
        );
    }

    #[test]
    fn network_classifier_buckets() {
        assert_eq!(
            classify_network_stderr("fatal: Could not resolve host github.com"),
            NetworkFault::Offline
        );
        assert_eq!(
            classify_network_stderr("ssh: connect to host port 22: Connection refused"),
            NetworkFault::Offline
        );
        assert_eq!(
            classify_network_stderr("fatal: Authentication failed for 'https://...'"),
            NetworkFault::Auth
        );
        assert_eq!(
            classify_network_stderr("git@github.com: Permission denied (publickey)."),
            NetworkFault::Auth
        );
        assert_eq!(
            classify_network_stderr("error: some refs could not be pushed"),
            NetworkFault::Other
        );
    }

    #[test]
    fn progress_lines_parse() {
        assert_eq!(
            parse_progress_line("Receiving objects:  45% (13/28), 1.20 MiB | 2.00 MiB/s"),
            Some(45)
        );
        assert_eq!(
            parse_progress_line("Resolving deltas: 100% (5/5), done."),
            Some(100)
        );
        assert_eq!(parse_progress_line("remote: Counting objects"), None);
        assert_eq!(parse_progress_line("Compressing objects: 101%"), None);
    }

    #[test]
    fn scp_shapes_are_not_confused_with_local_paths() {
        assert!(is_scp_like("git@github.com:acme/app.git"));
        assert!(is_scp_like("host:path"));
        assert!(!is_scp_like("/srv/git/app.git"));
        assert!(!is_scp_like("C:/repos/app"));
        assert!(!is_scp_like("relative/folder"));
    }
}

#[cfg(test)]
mod redaction_corpus_tests {
    use super::*;

    /// T14 redaction corpus: no password, token, query secret or fragment
    /// may survive `redact_url`, while routing info (user, host, path)
    /// stays readable for the operation log.
    #[test]
    fn corpus_never_leaks_secrets() {
        let cases = [
            "https://user:s3cret@github.com/acme/app.git",
            "https://oauth2:ghp_abcdef123456@github.com/acme/app.git",
            "https://user@s3cret@github.com/acme/app.git",
            "https://github.com/acme/app.git?token=s3cret#frag",
            "https://github.com/acme/app.git#frag",
            "git:user:pass@github.com:acme/app.git",
            "git@github.com:acme/app.git",
            "git://github.com/acme/app.git",
            "ssh://git@github.com:22/acme/app.git",
            "ssh://git:pass@github.com/acme/app.git",
            "/srv/git/app.git",
            "C:/repos/app",
            "../relative/app.git",
        ];
        for url in cases {
            let redacted = redact_url(url);
            for secret in [
                "s3cret",
                "ghp_abcdef123456",
                "token=",
                "?token",
                "#frag",
                ":pass@",
            ] {
                assert!(
                    !redacted.contains(secret),
                    "leak in {redacted:?} from {url:?}"
                );
            }
        }
    }

    #[test]
    fn corpus_keeps_routing_intact() {
        assert_eq!(
            redact_url("https://user:s3cret@github.com/acme/app.git"),
            "https://user@github.com/acme/app.git"
        );
        assert_eq!(
            redact_url("git@github.com:acme/app.git"),
            "git@github.com:acme/app.git"
        );
        assert_eq!(
            redact_url("https://github.com/acme/app.git"),
            "https://github.com/acme/app.git"
        );
        assert_eq!(redact_url("/srv/git/app.git"), "/srv/git/app.git");
    }
}
