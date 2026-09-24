//! Worktree status via `status --porcelain=v2 -z` (docs/05-git-engine.md §3).
//!
//! Byte-level parsing: records are NUL-terminated and paths are raw bytes,
//! never split on newlines or spaces, never lossy-decoded for identity.
//! Header lines stay LF-terminated even under `-z` and always precede entries.
//! Unknown `#` headers and unknown record types are skipped for forward
//! compatibility; structurally invalid known records are errors.

use std::path::Path;

use crate::git::runner::{GitRunner, RunError, READ_TIMEOUT};

/// Raw argv for a full worktree listing, branch headers included.
const STATUS_ARGV: &[&str] = &[
    "status",
    "--porcelain=v2",
    "-z",
    "--branch",
    "--untracked-files=all",
];

#[derive(Debug, PartialEq, Eq)]
pub enum StatusError {
    Run(RunError),
    GitFailed(String),
    Malformed(String),
}

impl From<RunError> for StatusError {
    fn from(value: RunError) -> Self {
        StatusError::Run(value)
    }
}

/// File kind derivable from mode bits alone. Text vs binary needs content
/// inspection and is resolved by the diff viewer (T08); status reports
/// `unknown` for regular files rather than guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Symlink,
    Submodule,
    Unknown,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Symlink => "symlink",
            FileKind::Submodule => "submodule",
            FileKind::Unknown => "unknown",
        }
    }
}

/// One parsed entry. Paths stay raw bytes; display conversion happens at the
/// DTO boundary and `path_id` tokens resolve back to these bytes server-side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFile {
    pub path: Vec<u8>,
    pub orig_path: Option<Vec<u8>>,
    /// Normalized index status: `A/M/D/R/C/T/U` or space (`.` becomes space).
    pub index_status: String,
    /// Normalized worktree status, `?` and `!` included.
    pub worktree_status: String,
    pub kind: FileKind,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadRef {
    Branch(String),
    Detached,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchInfo {
    /// None for `(initial)` on unborn branches or `(detached)` without commit.
    pub oid: Option<String>,
    pub head: Option<HeadRef>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedStatus {
    pub branch: BranchInfo,
    pub files: Vec<StatusFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCounts {
    pub staged: usize,
    pub unstaged: usize,
    pub conflicted: usize,
}

impl ParsedStatus {
    /// Staged = index status other than blank; unstaged = worktree status
    /// other than blank (untracked `?` counts); ignored `!` counts as neither.
    pub fn counts(&self) -> StatusCounts {
        let mut counts = StatusCounts {
            staged: 0,
            unstaged: 0,
            conflicted: 0,
        };
        for file in &self.files {
            if !matches!(file.index_status.as_str(), " " | "?" | "!") {
                counts.staged += 1;
            }
            if !matches!(file.worktree_status.as_str(), " " | "!") {
                counts.unstaged += 1;
            }
            if file.conflicted {
                counts.conflicted += 1;
            }
        }
        counts
    }
}

fn normalize_status(byte: u8) -> String {
    if byte == b'.' {
        " ".to_string()
    } else {
        String::from_utf8_lossy(&[byte]).into_owned()
    }
}

fn kind_from_modes(modes: &[&[u8]]) -> FileKind {
    if modes.iter().any(|m| *m == b"160000") {
        FileKind::Submodule
    } else if modes.iter().any(|m| *m == b"120000") {
        FileKind::Symlink
    } else {
        FileKind::Unknown
    }
}

fn split_fields(record: &[u8], fields: usize) -> Result<(Vec<&[u8]>, &[u8]), StatusError> {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(fields + 1);
    let mut rest = record;
    for _ in 0..fields {
        match rest.iter().position(|b| *b == b' ') {
            Some(i) => {
                parts.push(&rest[..i]);
                rest = &rest[i + 1..];
            }
            None => {
                return Err(StatusError::Malformed(format!(
                    "record has fewer than {fields} fields: {}",
                    String::from_utf8_lossy(record)
                )));
            }
        }
    }
    parts.push(rest);
    let path = parts.pop().unwrap_or(&[]);
    Ok((parts, path))
}

fn parse_head(branch: &mut BranchInfo, value: &[u8]) {
    if value == b"(detached)" {
        branch.head = Some(HeadRef::Detached);
    } else {
        branch.head = Some(HeadRef::Branch(String::from_utf8_lossy(value).into_owned()));
    }
}

fn parse_oid(branch: &mut BranchInfo, value: &[u8]) {
    if value != b"(initial)" {
        branch.oid = Some(String::from_utf8_lossy(value).into_owned());
    }
}

fn parse_upstream(branch: &mut BranchInfo, value: &[u8]) {
    branch.upstream = Some(String::from_utf8_lossy(value).into_owned());
}

fn parse_ahead_behind(branch: &mut BranchInfo, value: &[u8]) -> Result<(), StatusError> {
    let text = String::from_utf8_lossy(value);
    let mut parts = text.split_whitespace();
    let ahead = parts
        .next()
        .and_then(|s| s.strip_prefix('+'))
        .and_then(|s| s.parse::<u32>().ok());
    let behind = parts
        .next()
        .and_then(|s| s.strip_prefix('-'))
        .and_then(|s| s.parse::<u32>().ok());
    match (ahead, behind) {
        (Some(a), Some(b)) => {
            branch.ahead = Some(a);
            branch.behind = Some(b);
            Ok(())
        }
        _ => Err(StatusError::Malformed(format!("bad branch.ab: {text}"))),
    }
}

/// Parse raw `status --porcelain=v2 -z` stdout.
pub fn parse_status_v2(stdout: &[u8]) -> Result<ParsedStatus, StatusError> {
    let mut parsed = ParsedStatus::default();
    // NUL-separated chunks; a rename `2` record consumes the following chunk
    // as its verbatim origin path.
    let mut chunks = stdout.split(|b| *b == 0);
    while let Some(chunk) = chunks.next() {
        let mut record = chunk;
        // Headers are LF lines at the start of a chunk; entries never start
        // with `#`, so only leading `#` lines are headers.
        while let Some(stripped) = record.strip_prefix(b"#") {
            let _ = stripped;
            match record.iter().position(|b| *b == b'\n') {
                Some(i) => {
                    parse_header(&mut parsed.branch, &record[..i])?;
                    record = &record[i + 1..];
                }
                None => {
                    parse_header(&mut parsed.branch, record)?;
                    record = &[];
                }
            }
            if !record.starts_with(b"#") {
                break;
            }
        }
        if record.is_empty() {
            continue;
        }
        let kind = record[0];
        let body = &record[1..].strip_prefix(b" ").unwrap_or(record);
        match kind {
            b'1' => {
                let (fields, path) = split_fields(body, 7)?;
                let index = fields[0].first().copied().unwrap_or(b'.');
                let worktree = fields[0].get(1).copied().unwrap_or(b'.');
                parsed.files.push(StatusFile {
                    path: path.to_vec(),
                    orig_path: None,
                    index_status: normalize_status(index),
                    worktree_status: normalize_status(worktree),
                    kind: kind_from_modes(&[fields[2], fields[3], fields[4]]),
                    conflicted: index == b'U' || worktree == b'U',
                });
            }
            b'2' => {
                let (fields, path) = split_fields(body, 8)?;
                let index = fields[0].first().copied().unwrap_or(b'.');
                let worktree = fields[0].get(1).copied().unwrap_or(b'.');
                let orig = chunks.next().unwrap_or(&[]).to_vec();
                parsed.files.push(StatusFile {
                    path: path.to_vec(),
                    orig_path: Some(orig),
                    index_status: normalize_status(index),
                    worktree_status: normalize_status(worktree),
                    kind: kind_from_modes(&[fields[2], fields[3], fields[4]]),
                    conflicted: index == b'U' || worktree == b'U',
                });
            }
            b'u' => {
                let (fields, path) = split_fields(body, 9)?;
                parsed.files.push(StatusFile {
                    path: path.to_vec(),
                    orig_path: None,
                    index_status: normalize_status(fields[0].first().copied().unwrap_or(b'.')),
                    worktree_status: normalize_status(fields[0].get(1).copied().unwrap_or(b'.')),
                    kind: kind_from_modes(&[fields[2], fields[3], fields[4], fields[5]]),
                    conflicted: true,
                });
            }
            b'?' | b'!' => {
                parsed.files.push(StatusFile {
                    path: body.to_vec(),
                    orig_path: None,
                    index_status: " ".to_string(),
                    worktree_status: String::from_utf8_lossy(&[kind]).into_owned(),
                    kind: FileKind::Unknown,
                    conflicted: false,
                });
            }
            // Forward compatibility: ignore record types we do not know.
            _ => continue,
        }
    }
    Ok(parsed)
}

fn parse_header(branch: &mut BranchInfo, line: &[u8]) -> Result<(), StatusError> {
    let body = line.strip_prefix(b"#").unwrap_or(line);
    let body = body.strip_prefix(b" ").unwrap_or(body);
    let mut parts = body.splitn(2, |b| *b == b' ');
    let key = parts.next().unwrap_or(&[]);
    let value = parts.next().unwrap_or(&[]);
    match key {
        b"branch.oid" => parse_oid(branch, value),
        b"branch.head" => parse_head(branch, value),
        b"branch.upstream" => parse_upstream(branch, value),
        b"branch.ab" => parse_ahead_behind(branch, value)?,
        _ => {}
    }
    Ok(())
}

/// Display form of a raw path: lossy UTF-8 with U+FFFD for undecodable
/// bytes. Identity always stays in the raw bytes behind the path id; this
/// string is for rendering only and never round-trips into a mutation.
pub fn display_path(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).into_owned()
}

/// Convert parsed entries into IPC rows. Path ids come from the registry
/// listing cached alongside this read, so they always match the current
/// generation; callers must `status_put` first, then `status_path_id`.
pub fn to_display_rows(
    parsed: &ParsedStatus,
) -> Vec<(String, Option<String>, String, String, String, bool)> {
    parsed
        .files
        .iter()
        .map(|file| {
            (
                display_path(&file.path),
                file.orig_path.as_ref().map(|raw| display_path(raw)),
                file.index_status.clone(),
                file.worktree_status.clone(),
                file.kind.as_str().to_string(),
                file.conflicted,
            )
        })
        .collect()
}

/// Run status in `cwd` and parse it. `GIT_OPTIONAL_LOCKS=0` avoids optional
/// index refresh writes on this read-only path (docs/05-git-engine.md §9).
pub async fn read_status(runner: &GitRunner, cwd: &Path) -> Result<ParsedStatus, StatusError> {
    let output = runner
        .run_with_env(
            cwd,
            STATUS_ARGV,
            READ_TIMEOUT,
            &[("GIT_OPTIONAL_LOCKS", "0")],
        )
        .await?;
    if !output.success {
        let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        stderr.truncate(500);
        return Err(StatusError::GitFailed(stderr));
    }
    parse_status_v2(&output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::process::Command as StdCommand;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("gitdock-t07-{}-{id}-{label}", std::process::id()));
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
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@x")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@x")
            .status()
            .expect("spawn git");
        assert!(status.success(), "git {args:?} failed in {}", cwd.display());
    }

    fn chunk(record: &[u8]) -> Vec<u8> {
        let mut out = record.to_vec();
        out.push(0);
        out
    }

    fn headers() -> Vec<u8> {
        b"# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n# unknown.future something\n".to_vec()
    }

    #[test]
    fn headers_parse_branch_upstream_ahead_behind() {
        let parsed = parse_status_v2(&headers()).expect("parse");
        assert_eq!(parsed.branch.oid.as_deref(), Some("abc123"));
        assert_eq!(
            parsed.branch.head,
            Some(HeadRef::Branch("main".to_string()))
        );
        assert_eq!(parsed.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!(parsed.branch.ahead, Some(2));
        assert_eq!(parsed.branch.behind, Some(1));
        assert!(parsed.files.is_empty());
    }

    #[test]
    fn unborn_and_detached_heads() {
        let parsed =
            parse_status_v2(b"# branch.oid (initial)\n# branch.head main\n").expect("parse");
        assert_eq!(parsed.branch.oid, None);
        assert_eq!(
            parsed.branch.head,
            Some(HeadRef::Branch("main".to_string()))
        );

        let parsed =
            parse_status_v2(b"# branch.oid abc123\n# branch.head (detached)\n").expect("parse");
        assert_eq!(parsed.branch.head, Some(HeadRef::Detached));
    }

    #[test]
    fn ordinary_entry_with_spaces_and_staged_unstaged() {
        let mut raw = headers();
        raw.extend(chunk(
            b"1 MM N... 100644 100644 100644 abcdef1 abcdef2 my dir/a b.txt",
        ));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files.len(), 1);
        let file = &parsed.files[0];
        assert_eq!(file.path, b"my dir/a b.txt");
        assert_eq!(file.index_status, "M");
        assert_eq!(file.worktree_status, "M");
        assert!(!file.conflicted);
        assert_eq!(file.kind, FileKind::Unknown);
        assert_eq!(file.orig_path, None);
        let counts = parsed.counts();
        assert_eq!(
            counts,
            StatusCounts {
                staged: 1,
                unstaged: 1,
                conflicted: 0
            }
        );
    }

    #[test]
    fn rename_keeps_both_raw_paths_with_newline() {
        let mut raw = headers();
        raw.extend(chunk(
            b"2 R. N... 100644 100644 100644 abcdef1 abcdef2 R100 new\nline.txt",
        ));
        raw.extend(chunk(b"old.txt"));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files.len(), 1);
        let file = &parsed.files[0];
        assert_eq!(file.path, b"new\nline.txt");
        assert_eq!(file.orig_path.as_deref(), Some(b"old.txt".as_slice()));
        assert_eq!(file.index_status, "R");
    }

    #[test]
    fn non_utf8_paths_stay_raw_and_untracked_marked() {
        let mut raw = headers();
        let mut record = b"1 A. N... 100644 100644 100644 abcdef1 abcdef2 ".to_vec();
        record.extend([0xff, 0xfe, b'.', b't', b'x', b't']);
        raw.extend(chunk(&record));
        raw.extend(chunk(b"? plain-untracked.txt"));
        raw.extend(chunk(b"! ignored.tmp"));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files.len(), 3);
        assert_eq!(
            parsed.files[0].path,
            vec![0xff, 0xfe, b'.', b't', b'x', b't']
        );
        assert_eq!(parsed.files[1].worktree_status, "?");
        assert_eq!(parsed.files[1].index_status, " ");
        assert_eq!(parsed.files[2].worktree_status, "!");
        let counts = parsed.counts();
        // Added is staged; untracked is unstaged; ignored is neither.
        assert_eq!(counts.staged, 1);
        assert_eq!(counts.unstaged, 1);
        assert_eq!(counts.conflicted, 0);
    }

    #[test]
    fn unmerged_entries_are_conflicted() {
        let mut raw = headers();
        raw.extend(chunk(
            b"u UU N... 100644 100644 100644 100644 aaa111 bbb222 ccc333 both.txt",
        ));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files.len(), 1);
        assert!(parsed.files[0].conflicted);
        assert_eq!(parsed.counts().conflicted, 1);
    }

    #[test]
    fn symlink_submodule_kinds_from_modes() {
        let mut raw = headers();
        raw.extend(chunk(b"1 A. N... 120000 120000 000000 aaa111 bbb222 link"));
        raw.extend(chunk(
            b"1 M. N... 160000 160000 160000 aaa111 bbb222 vendor",
        ));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files[0].kind, FileKind::Symlink);
        assert_eq!(parsed.files[1].kind, FileKind::Submodule);
    }

    #[test]
    fn unknown_record_types_are_skipped() {
        let mut raw = headers();
        raw.extend(chunk(b"9 ?? future format here"));
        raw.extend(chunk(b"? kept.txt"));
        let parsed = parse_status_v2(&raw).expect("parse");
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].path, b"kept.txt");
    }

    #[test]
    fn malformed_records_are_errors() {
        assert!(matches!(
            parse_status_v2(b"1 XY\n"),
            Err(StatusError::Malformed(_))
        ));
        assert!(matches!(
            parse_status_v2(b"# branch.ab nope\n"),
            Err(StatusError::Malformed(_))
        ));
    }

    fn v1_oracle(repo: &Path) -> Vec<(String, String, String)> {
        // Independent oracle: porcelain v1 with quoting off.
        let output = StdCommand::new("git")
            .current_dir(repo)
            .args([
                "-c",
                "core.quotepath=off",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--branch",
            ])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("oracle git");
        assert!(output.status.success());
        let mut entries = Vec::new();
        let mut chunks = output.stdout.split(|b| *b == 0).peekable();
        while let Some(chunk) = chunks.next() {
            if chunk.is_empty() || chunk.starts_with(b"#") {
                continue;
            }
            let xy = String::from_utf8_lossy(&chunk[..2]).into_owned();
            let mut path = String::from_utf8_lossy(&chunk[3..]).into_owned();
            if xy.starts_with('R') || xy.starts_with('C') {
                // v1 -z rename: entry then origin chunk.
                let orig = chunks.next().unwrap_or(&[]);
                let _ = orig;
                if let Some((new, _old)) = path.split_once(" -> ") {
                    path = new.to_string();
                }
            }
            // v1 `??` means untracked: no index status, worktree `?`.
            let (x, y) = if xy == "??" {
                (" ".to_string(), "?".to_string())
            } else {
                (
                    xy.chars()
                        .next()
                        .unwrap_or(' ')
                        .to_string()
                        .replace('.', " "),
                    xy.chars()
                        .nth(1)
                        .unwrap_or(' ')
                        .to_string()
                        .replace('.', " "),
                )
            };
            entries.push((path, x, y));
        }
        entries.sort();
        entries
    }

    #[tokio::test]
    async fn live_repo_matches_v1_oracle() {
        let root = temp_root("live");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        std::fs::write(repo.join("del.txt"), "d\n").expect("write");
        git(&repo, &["add", "a.txt", "del.txt"]);
        git(&repo, &["commit", "-m", "second"]);
        // Same file staged and unstaged.
        std::fs::write(repo.join("a.txt"), "a\nstaged\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        std::fs::write(repo.join("a.txt"), "a\nstaged\nunstaged\n").expect("write");
        // Delete tracked, add symlink + gitlink + weird untracked names.
        std::fs::remove_file(repo.join("del.txt")).expect("remove");
        std::os::unix::fs::symlink("a.txt", repo.join("link")).expect("symlink");
        git(&repo, &["add", "link"]);
        git(
            &repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,4b825dcb2362799c7d7a3438421c82cda77cb53d,vendor",
            ],
        );
        std::fs::write(repo.join("sp ace.txt"), "s\n").expect("write");
        let raw_name = std::ffi::OsStr::from_bytes(b"raw-\xff.txt");
        std::fs::write(repo.join(raw_name), "r\n").expect("write raw");

        let exe = GitRunner::resolve_from_path().expect("system git");
        let parsed = read_status(&GitRunner::new(exe), &repo)
            .await
            .expect("read_status");

        let mut actual: Vec<(String, String, String)> = parsed
            .files
            .iter()
            .map(|f| {
                (
                    String::from_utf8_lossy(&f.path).into_owned(),
                    f.index_status.clone(),
                    f.worktree_status.clone(),
                )
            })
            .collect();
        actual.sort();
        assert_eq!(actual, v1_oracle(&repo));

        // Same-file staged+unstaged entry is one row with both flags.
        let a = parsed
            .files
            .iter()
            .find(|f| f.path == b"a.txt")
            .expect("a.txt");
        assert_eq!(a.index_status, "M");
        assert_eq!(a.worktree_status, "M");

        let kinds: std::collections::HashMap<Vec<u8>, FileKind> = parsed
            .files
            .iter()
            .map(|f| (f.path.clone(), f.kind))
            .collect();
        assert_eq!(kinds[b"link".as_slice()], FileKind::Symlink);
        assert_eq!(kinds[b"vendor".as_slice()], FileKind::Submodule);
        assert!(parsed
            .files
            .iter()
            .any(|f| f.path == b"raw-\xff.txt".as_slice()));
    }

    #[tokio::test]
    async fn index_lock_is_tolerated_and_preserved() {
        // GIT_OPTIONAL_LOCKS=0 means status never takes the index lock, so a
        // stale lock from a crashed operation must not break worktree reads —
        // and we must never delete a lock we do not own.
        let root = temp_root("lock");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("dirty.txt"), "dirty\n").expect("write");
        git(&repo, &["add", "dirty.txt"]);
        let lock = repo.join(".git").join("index.lock");
        std::fs::write(&lock, "held").expect("lock");
        let exe = GitRunner::resolve_from_path().expect("system git");
        let parsed = read_status(&GitRunner::new(exe), &repo)
            .await
            .expect("status tolerates a stale index.lock");
        assert!(parsed.files.iter().any(|f| f.path == b"dirty.txt"));
        assert!(lock.exists(), "index.lock must be preserved");
        std::fs::remove_file(&lock).expect("cleanup");
    }
}
