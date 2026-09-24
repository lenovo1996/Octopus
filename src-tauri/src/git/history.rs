//! History and ref readers (docs/05-git-engine.md §3).
//!
//! Topology comes from `rev-list --topo-order --parents` on pinned tips;
//! metadata from a length-framed `cat-file --batch` parse (never a separator
//! that can appear inside a message). Ref names cannot contain ASCII control
//! characters per `git check-ref-format`, so line splitting is safe there.

use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

use crate::domain::{AppError, ErrorCode, RecoveryAction, RefItem};
use crate::git::runner::{GitRunner, READ_TIMEOUT};
use crate::services::RepoSession;

fn engine_err(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::GIT_ERROR, message, RecoveryAction::Refresh, true)
}

fn lines_of(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

// ---- Refs ----

/// Machine-readable refs with annotated tags peeled to their commit.
pub async fn list_refs(
    runner: &GitRunner,
    session: &RepoSession,
) -> Result<Vec<RefItem>, AppError> {
    let cwd = &session.worktree_root;
    let out = runner
        .run(
            cwd,
            &[
                "for-each-ref",
                "--format=%(objectname) %00 %(objecttype) %00 %(*objectname) %00 %(refname)",
            ],
            READ_TIMEOUT,
        )
        .await
        .map_err(|_| engine_err("Failed to read refs"))?;
    if !out.success {
        return Err(engine_err("Failed to read refs"));
    }
    // Current HEAD target for the `current` flag.
    let head_out = runner
        .run(cwd, &["symbolic-ref", "-q", "HEAD"], READ_TIMEOUT)
        .await
        .map_err(|_| engine_err("Failed to read HEAD"))?;
    let head_ref = head_out
        .success
        .then(|| String::from_utf8_lossy(&head_out.stdout).trim().to_string());

    // Worktrees other than this one, for `checkedOutElsewhere`.
    let mut elsewhere: Vec<String> = Vec::new();
    if let Ok(wt) = runner
        .run(cwd, &["worktree", "list", "--porcelain"], READ_TIMEOUT)
        .await
    {
        if wt.success {
            let mut path = String::new();
            for line in String::from_utf8_lossy(&wt.stdout).lines() {
                if let Some(p) = line.strip_prefix("worktree ") {
                    path = p.trim().to_string();
                } else if let Some(b) = line.strip_prefix("branch ") {
                    let branch = b.trim().to_string();
                    let here = Path::new(&path) == session.worktree_root.as_path();
                    if !here && !branch.is_empty() {
                        elsewhere.push(branch);
                    }
                }
            }
        }
    }

    let mut refs = Vec::new();
    for line in lines_of(&out.stdout) {
        // Fields are separated by " \0 " (space, NUL, space).
        let fields: Vec<&str> = line.split(" \0 ").collect();
        if fields.len() != 4 {
            continue;
        }
        let (object, objtype, peeled, full) = (
            fields[0].trim(),
            fields[1].trim(),
            fields[2].trim(),
            fields[3].trim(),
        );
        let (kind, label) = if let Some(rest) = full.strip_prefix("refs/heads/") {
            ("local", rest.to_string())
        } else if let Some(rest) = full.strip_prefix("refs/remotes/") {
            ("remote", rest.to_string())
        } else if let Some(rest) = full.strip_prefix("refs/tags/") {
            ("tag", rest.to_string())
        } else {
            continue;
        };
        let oid = if objtype == "tag" && !peeled.is_empty() {
            peeled
        } else {
            object
        }
        .to_string();
        refs.push(RefItem {
            // Interim identity: full ref name. Opaque token mapping arrives
            // with the mutation commands (T09+).
            ref_id: full.to_string(),
            full_name: full.to_string(),
            label,
            kind: kind.to_string(),
            oid,
            current: head_ref.as_deref() == Some(full),
            checked_out_elsewhere: elsewhere.iter().any(|b| b == full),
        });
    }
    Ok(refs)
}

// ---- Topology ----

#[derive(Debug, Clone)]
pub struct TopoRow {
    pub oid: String,
    pub parents: Vec<String>,
    pub boundary: bool,
}

/// Cap on cached topology rows. `rev-list` output grows ~130 bytes/row,
/// so this keeps one walk well under the runner output cap even for
/// monorepos (a 223k-commit walk is ~24MB and would trip OUTPUT_LIMIT).
pub const MAX_TOPO_ROWS: usize = 20_000;

/// Batch size for `cat-file --batch` metadata reads. One 2000-commit
/// batch stays far under the runner output cap; callers with more oids
/// are split transparently.
const METADATA_BATCH: usize = 2_000;

/// Full topo-ordered topology for pinned tips. Children before parents;
/// timestamps never substitute topology. Returns the rows plus whether
/// the walk was cut at [`MAX_TOPO_ROWS`] (oldest commits omitted).
pub async fn read_topology(
    runner: &GitRunner,
    session: &RepoSession,
    tips: &[String],
) -> Result<(Vec<TopoRow>, bool), AppError> {
    read_topology_capped(runner, session, tips, MAX_TOPO_ROWS).await
}

/// [`read_topology`] with an explicit row cap. Requests one row past the
/// cap; seeing it means the walk is truncated.
pub async fn read_topology_capped(
    runner: &GitRunner,
    session: &RepoSession,
    tips: &[String],
    max_rows: usize,
) -> Result<(Vec<TopoRow>, bool), AppError> {
    if tips.is_empty() {
        return Ok((Vec::new(), false));
    }
    let over = max_rows.saturating_add(1).to_string();
    let mut argv: Vec<&str> = vec![
        "rev-list",
        "--topo-order",
        "--parents",
        "--boundary",
        "--max-count",
        &over,
    ];
    let tip_refs: Vec<&str> = tips.iter().map(String::as_str).collect();
    argv.extend(tip_refs);
    let out = runner
        .run(&session.worktree_root, &argv, READ_TIMEOUT)
        .await
        .map_err(|_| engine_err("Failed to read history"))?;
    if !out.success {
        return Err(engine_err("Failed to read history"));
    }
    // Shallow graft points (`$GIT_DIR/shallow`) have no `-` marker from
    // rev-list --boundary when reachable; detect them from the shallow file.
    let mut shallow: std::collections::HashSet<String> = std::collections::HashSet::new();
    for dir in [&session.git_dir, &session.common_dir] {
        if let Ok(content) = std::fs::read_to_string(dir.join("shallow")) {
            shallow.extend(content.lines().map(|l| l.trim().to_string()));
        }
    }
    let mut rows = Vec::new();
    // Boundary markers (`-`) ride along free: `--max-count` limits real
    // commits only, so the truncation decision counts those, never markers.
    let mut real = 0usize;
    for line in lines_of(&out.stdout) {
        let (dash, rest) = match line.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, line.as_str()),
        };
        let mut parts = rest.split_whitespace();
        let Some(oid) = parts.next() else { continue };
        if !dash {
            real += 1;
        }
        rows.push(TopoRow {
            boundary: dash || shallow.contains(oid),
            oid: oid.to_string(),
            parents: parts.map(str::to_string).collect(),
        });
    }
    // One real commit past the cap was requested: its presence proves the
    // walk is longer than what fits in the session cache.
    let truncated = real > max_rows;
    rows.truncate(max_rows);
    Ok((rows, truncated))
}

// ---- Commit objects (length-framed, separator-free) ----

#[derive(Debug, Clone)]
pub struct CommitMeta {
    pub oid: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub authored_at: String,
    pub committed_at: String,
    pub subject: String,
    pub body: String,
}

fn parse_commit_object(oid: &str, bytes: &[u8]) -> Option<CommitMeta> {
    let split = bytes.windows(2).position(|w| w == b"\n\n")?;
    let (header, message) = (&bytes[..split], &bytes[split + 2..]);
    let header = String::from_utf8_lossy(header);
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    for line in header.lines() {
        if let Some(p) = line.strip_prefix("parent ") {
            parents.push(p.trim().to_string());
        } else if let Some(a) = line.strip_prefix("author ") {
            author = Some(a.to_string());
        } else if let Some(c) = line.strip_prefix("committer ") {
            committer = Some(c.to_string());
        }
    }
    let message = String::from_utf8_lossy(message).into_owned();
    let subject = message.lines().next().unwrap_or("").to_string();
    // Body excludes the subject, its separating blank line(s), and trailing
    // newlines (git cleanup semantics); interior blank lines are preserved.
    let body = message
        .strip_prefix(subject.as_str())
        .unwrap_or("")
        .trim_start_matches('\n')
        .trim_end_matches('\n')
        .to_string();
    Some(CommitMeta {
        oid: oid.to_string(),
        parents,
        author_name: author.as_deref().map(person_name).unwrap_or_default(),
        authored_at: author.as_deref().map(person_iso).unwrap_or_default(),
        committed_at: committer.as_deref().map(person_iso).unwrap_or_default(),
        subject,
        body,
    })
}

/// `Name <email> ts tz` -> name part (drops the `<email>` suffix).
fn person_name(field: &str) -> String {
    match field.rfind(" <") {
        Some(i) => field[..i].to_string(),
        None => field
            .rsplit_once(' ')
            .map(|(n, _)| n)
            .unwrap_or(field)
            .to_string(),
    }
}

/// `Name <email> ts tz` -> ISO 8601 `YYYY-MM-DDTHH:MM:SS+HH:MM`.
fn person_iso(field: &str) -> String {
    let mut parts = field.rsplit(' ');
    let tz = parts.next().unwrap_or("+0000");
    let ts: i64 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    iso_from_unix(ts, tz)
}

fn iso_from_unix(ts: i64, tz: &str) -> String {
    let sign = if tz.starts_with('-') { -1 } else { 1 };
    let digits = tz.trim_start_matches(['+', '-']);
    let (hh, mm) = digits
        .split_at_checked(2)
        .and_then(|(h, m)| Some((h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)))
        .unwrap_or((0, 0));
    let off = sign * (hh * 3600 + mm * 60);
    let (date, time) = civil_from_days(ts + off);
    format!("{date}T{time}{tz}")
}

fn div_floor(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

/// Shifted Unix seconds -> (YYYY-MM-DD, HH:MM:SS); Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (String, String) {
    let day_secs = z.rem_euclid(86400);
    // Day count, then shift to days since civil 0000-03-01.
    let mut days = div_floor(z, 86400) + 719468;
    let era = div_floor(days, 146097);
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    // Continue from day-of-era, not the absolute day count.
    days = doe - (yoe * 365 + yoe / 4 - yoe / 100);
    let mp = (5 * days + 2) / 153;
    let d = days - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let (hh, rem) = (day_secs / 3600, day_secs % 3600);
    let (mi, ss) = (rem / 60, rem % 60);
    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{hh:02}:{mi:02}:{ss:02}"),
    )
}

/// Batch metadata for `oids`, preserving input order. Missing objects are skipped.
pub async fn read_metadata(
    runner: &GitRunner,
    session: &RepoSession,
    oids: &[String],
) -> Result<Vec<CommitMeta>, AppError> {
    if oids.is_empty() {
        return Ok(Vec::new());
    }
    // One `cat-file --batch` per chunk: a full-history search fans out to
    // tens of thousands of oids, and a single batch would trip the runner
    // output cap. Order is preserved across chunks.
    let mut metas = Vec::with_capacity(oids.len());
    for chunk in oids.chunks(METADATA_BATCH) {
        metas.extend(read_metadata_chunk(runner, session, chunk).await?);
    }
    Ok(metas)
}

async fn read_metadata_chunk(
    runner: &GitRunner,
    session: &RepoSession,
    oids: &[String],
) -> Result<Vec<CommitMeta>, AppError> {
    // `cat-file --batch` reads oid list from stdin; stream it to avoid argv limits.
    let mut command = tokio::process::Command::new(runner.exe());
    command
        .current_dir(&session.worktree_root)
        .args(["cat-file", "--batch"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for key in crate::git::runner::SANITIZED_GIT_ENV {
        command.env_remove(key);
    }
    let mut child = command
        .spawn()
        .map_err(|_| engine_err("Failed to read objects"))?;
    // Feed stdin concurrently: sequential write-then-read deadlocks once
    // the oid list outgrows the pipe buffer (large-repo search), with the
    // child blocked on full stdout while we block on full stdin.
    let input = oids.join("\n") + "\n";
    let stdin_taken = child.stdin.take();
    let feed = tokio::spawn(async move {
        if let Some(mut stdin) = stdin_taken {
            stdin
                .write_all(input.as_bytes())
                .await
                .map_err(|_| engine_err("Failed to read objects"))?;
            stdin
                .shutdown()
                .await
                .map_err(|_| engine_err("Failed to read objects"))?;
        }
        Ok::<(), AppError>(())
    });
    let output = tokio::time::timeout(crate::git::runner::READ_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| engine_err("Object read timed out"))?
        .map_err(|_| engine_err("Failed to read objects"))?;
    feed.await
        .map_err(|_| engine_err("Failed to read objects"))??;
    if output.stdout.len() > crate::git::runner::MAX_OUTPUT_BYTES {
        return Err(AppError::new(
            ErrorCode::OUTPUT_LIMIT,
            "History page exceeds output limits",
            RecoveryAction::RetryRead,
            true,
        ));
    }
    Ok(parse_batch(&output.stdout))
}

fn parse_batch(stdout: &[u8]) -> Vec<CommitMeta> {
    let mut metas = Vec::new();
    let mut rest = stdout;
    while !rest.is_empty() {
        let nl = match rest.iter().position(|&b| b == b'\n') {
            Some(i) => i,
            None => break,
        };
        let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        let mut parts = header.split_whitespace();
        let (Some(oid), Some(kind), Some(size)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if kind == "missing" {
            continue;
        }
        let Ok(size) = size.parse::<usize>() else {
            continue;
        };
        if rest.len() < size {
            break;
        }
        let (content, tail) = rest.split_at(size);
        rest = tail.strip_prefix(b"\n").unwrap_or(tail);
        if kind == "commit" {
            if let Some(meta) = parse_commit_object(oid, content) {
                metas.push(meta);
            }
        }
    }
    metas
}

// ---- Commit file changes ----

#[derive(Debug, Clone)]
pub struct ChangedPath {
    pub status: String,
    pub path: String,
    pub old_path: Option<String>,
    /// Raw bytes behind the display strings; diff invocation and path
    /// tokens use these so non-UTF-8 paths stay exact (T08).
    pub raw_path: Vec<u8>,
    pub raw_old_path: Option<Vec<u8>>,
}

/// Normalized name-status list for `oid` vs the chosen parent.
/// `parent_index=None` is only valid for the root commit.
pub async fn read_commit_files(
    runner: &GitRunner,
    session: &RepoSession,
    oid: &str,
    parent_index: Option<usize>,
    parents: &[String],
) -> Result<(Option<usize>, Vec<ChangedPath>), AppError> {
    let cwd = &session.worktree_root;
    // Owned argv so borrows stay alive across await.
    let mut owned: Vec<String> = vec![
        "diff-tree".into(),
        "--no-commit-id".into(),
        "--name-status".into(),
        "-z".into(),
        "-r".into(),
    ];
    let used_parent: Option<usize> = match (parent_index, parents) {
        (None, []) => {
            owned.push("--root".into());
            owned.push(oid.to_string());
            None
        }
        // Contract (docs/04-ipc-contracts.md): parent index defaults to 0
        // for every non-root commit, merges included; null is only valid
        // for the root. The UI offers the parent dropdown after this
        // first load, so refusing here would strand merge details.
        (None, _) => {
            owned.push(parents[0].clone());
            owned.push(oid.to_string());
            Some(0)
        }
        (Some(i), _) if i >= parents.len() => {
            return Err(AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "Parent index is out of range",
                RecoveryAction::InspectState,
                false,
            ));
        }
        (Some(i), _) => {
            owned.push(parents[i].clone());
            owned.push(oid.to_string());
            Some(i)
        }
    };
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let out = runner
        .run(cwd, &refs, READ_TIMEOUT)
        .await
        .map_err(|_| engine_err("Failed to read commit files"))?;
    if !out.success {
        return Err(engine_err("Failed to read commit files"));
    }
    Ok((
        used_parent.or((!parents.is_empty()).then_some(0)),
        parse_name_status(&out.stdout),
    ))
}

/// Parse `-z` name-status records. NUL-framed; renames carry a second path.
fn parse_name_status(stdout: &[u8]) -> Vec<ChangedPath> {
    let mut files = Vec::new();
    let mut records = stdout.split(|&b| b == 0).peekable();
    while let Some(status) = records.next() {
        if status.is_empty() {
            continue;
        }
        let status = String::from_utf8_lossy(status).into_owned();
        let kind = status.chars().next().unwrap_or(' ');
        let first = records.next();
        let second = if kind == 'R' || kind == 'C' {
            records.next()
        } else {
            None
        };
        // Rename records carry OLD then NEW; single-path records carry the path.
        let ((raw_path, raw_old_path), (path, old_path)) = match (first, second) {
            (Some(old), Some(new)) => (
                (new.to_vec(), Some(old.to_vec())),
                (
                    String::from_utf8_lossy(new).into_owned(),
                    Some(String::from_utf8_lossy(old).into_owned()),
                ),
            ),
            (Some(p), None) => (
                (p.to_vec(), None),
                (String::from_utf8_lossy(p).into_owned(), None),
            ),
            _ => continue,
        };
        files.push(ChangedPath {
            status: kind.to_string(),
            path,
            old_path,
            raw_path,
            raw_old_path,
        });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_merge_commit_objects() {
        let raw = b"tree abc\nparent 111\nparent 222\nauthor A U <a@x> 1700000000 +0700\ncommitter C V <c@x> 1700000100 +0700\n\nFirst subject\n\nBody line\n";
        let meta = parse_commit_object("deadbeef", raw).expect("parse");
        assert_eq!(meta.parents, vec!["111", "222"]);
        assert_eq!(meta.author_name, "A U");
        assert_eq!(meta.subject, "First subject");
        assert_eq!(meta.body, "Body line");
        assert_eq!(meta.authored_at, "2023-11-15T05:13:20+0700");
        assert_eq!(meta.committed_at, "2023-11-15T05:15:00+0700");
    }

    #[test]
    fn timestamp_formats_spot_check() {
        assert_eq!(iso_from_unix(0, "+0000"), "1970-01-01T00:00:00+0000");
        assert_eq!(iso_from_unix(0, "-0500"), "1969-12-31T19:00:00-0500");
        // Non-epoch date with a positive offset crossing midnight UTC.
        assert_eq!(
            iso_from_unix(1700000000, "+0700"),
            "2023-11-15T05:13:20+0700"
        );
    }

    #[test]
    fn parses_rename_and_plain_name_status() {
        let raw = b"M\0a.txt\0R100\0old.txt\0new.txt\0";
        let files = parse_name_status(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].status, "M");
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[1].status, "R");
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].old_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn skips_malformed_batch_records() {
        let raw = b"deadbeef missing\nabc commit 3\nxyz";
        assert!(parse_batch(raw).is_empty());
    }
}
