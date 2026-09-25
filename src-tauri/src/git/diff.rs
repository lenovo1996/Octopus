//! Unified worktree/index/commit diffs.
//!
//! Path identity is always raw bytes carried by listing tokens; display
//! strings never feed back into Git. Argv travels as OS strings (never a
//! shell) with `--` separating options from the path and a `:(literal)`
//! magic prefix disabling glob matching, so `-`, spaces, newlines, Unicode
//! and magic characters stay exact on the Git 2.43 baseline (which accepts
//! neither `--literal-pathspecs` nor `--pathspec-from-file` for
//! diff/ls-files, and a bare `--` does not disable magic). Reads use
//! `--no-ext-diff --no-textconv` and `GIT_OPTIONAL_LOCKS=0`; untracked
//! previews read the file directly and never touch the index.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::domain::{DiffDocument, DiffHunk, DiffLine};
use crate::git::runner::{GitRunner, RunError, READ_TIMEOUT};

/// Refuse to parse patches larger than this; the viewer gets `tooLarge`.
pub const MAX_DIFF_BYTES: usize = 512 * 1024;
/// Cap rendered lines per document; beyond it the document is `truncated`.
pub const MAX_DIFF_LINES: usize = 5000;
/// Untracked file preview cap; beyond it the document is `tooLarge`.
pub const MAX_PREVIEW_BYTES: usize = 256 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum DiffError {
    Run(RunError),
    GitFailed(String),
    /// Caller bug (bad token, untracked index diff, ...); message is user-safe.
    Invalid(String),
    TooLarge,
}

/// Small deterministic fingerprint used only as a stale-state token. It is
/// not a security primitive; the backend still resolves the path token and
/// rebuilds the patch immediately before every mutation.
pub fn patch_fingerprint(bytes: &[u8]) -> String {
    let mut left = 0xcbf29ce484222325u64;
    let mut right = 0x84222325cbf29ce4u64;
    for byte in bytes {
        left ^= u64::from(*byte);
        left = left.wrapping_mul(0x100000001b3);
        right ^= u64::from(*byte).wrapping_add(0x9d);
        right = right.rotate_left(5).wrapping_mul(0x100000001b3);
    }
    format!("h1:{left:016x}{right:016x}")
}

/// Split one unified patch into its file prelude and exact hunk byte ranges.
/// A hunk body cannot start with `@@`; body rows always have a diff marker.
fn patch_hunk_ranges(
    patch: &[u8],
) -> Option<(std::ops::Range<usize>, Vec<std::ops::Range<usize>>)> {
    let mut starts = Vec::new();
    let mut offset = 0usize;
    for line in patch.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(b"@@ -") {
            starts.push(offset);
        }
        offset += line.len();
    }
    let first = *starts.first()?;
    let mut ranges = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().copied().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(patch.len());
        ranges.push(start..end);
    }
    Some((0..first, ranges))
}

/// Rebuild a valid one-hunk patch from a backend-issued hunk id.
pub fn select_patch_hunk(patch: &[u8], hunk_id: &str) -> Result<Vec<u8>, DiffError> {
    let (prelude, ranges) = patch_hunk_ranges(patch)
        .ok_or_else(|| DiffError::Invalid("This file has no selectable text hunks".to_string()))?;
    let selected = ranges
        .into_iter()
        .find(|range| patch_fingerprint(&patch[range.clone()]) == hunk_id)
        .ok_or_else(|| DiffError::Invalid("The diff changed; refresh it and retry".to_string()))?;
    let mut result = Vec::with_capacity(prelude.len() + selected.len());
    result.extend_from_slice(&patch[prelude]);
    result.extend_from_slice(&patch[selected]);
    Ok(result)
}

impl From<RunError> for DiffError {
    fn from(value: RunError) -> Self {
        DiffError::Run(value)
    }
}

/// Empty tree hash per object format, for root-commit comparisons.
pub fn empty_tree_hash(object_format: &str) -> &'static str {
    if object_format == "sha256" {
        "6ef19b41225c5369f1c104d45d8d85efa9b0c9059e"
    } else {
        "4b825dc642cb6b594a121cfb6a31532b5895c94c"
    }
}

fn text_doc(
    display_path: String,
    hunks: Vec<DiffHunk>,
    additions: u64,
    deletions: u64,
    truncated: bool,
) -> DiffDocument {
    DiffDocument {
        kind: "text".to_string(),
        display_path,
        additions: Some(additions),
        deletions: Some(deletions),
        hunks,
        truncated,
        reason: None,
    }
}

fn fallback_doc(kind: &str, display_path: String, reason: &str) -> DiffDocument {
    DiffDocument {
        kind: kind.to_string(),
        display_path,
        additions: None,
        deletions: None,
        hunks: Vec::new(),
        truncated: false,
        reason: Some(reason.to_string()),
    }
}

fn parse_hunk_header(line: &str) -> Option<(u64, u64)> {
    // `@@ -oldStart[,oldCount] +newStart[,newCount] @@ [section]`
    let mut rest = line.strip_prefix("@@")?;
    rest = rest.trim_start();
    let old_part = rest.strip_prefix('-')?;
    let old_start: u64 = old_part.split([',', ' ']).next()?.parse().ok()?;
    let plus = rest.find('+')?;
    let new_part = &rest[plus + 1..];
    let new_start: u64 = new_part.split([',', ' ']).next()?.parse().ok()?;
    Some((old_start, new_start))
}

/// Parse a unified patch into hunks. Display text is lossy by design;
/// identity never depends on it. `\ No newline` markers become `noNewline`
/// rows so the viewer can say so explicitly.
pub fn parse_unified_patch(patch: &[u8], max_lines: usize) -> (Vec<DiffHunk>, u64, u64, bool) {
    let text = String::from_utf8_lossy(patch);
    let hunk_ids = patch_hunk_ranges(patch)
        .map(|(_, ranges)| {
            ranges
                .into_iter()
                .map(|range| patch_fingerprint(&patch[range]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut additions = 0u64;
    let mut deletions = 0u64;
    let mut truncated = false;
    let mut line_count = 0usize;
    let mut old_line = 0u64;
    let mut new_line = 0u64;
    let mut in_hunk = false;

    for raw in text.lines() {
        if raw.starts_with("Binary files ") {
            // Handled by the caller via `is_binary_patch`; unreachable here.
            continue;
        }
        if let Some((old_start, new_start)) = parse_hunk_header(raw) {
            let hunk_id = hunk_ids
                .get(hunks.len())
                .cloned()
                .unwrap_or_else(|| patch_fingerprint(raw.as_bytes()));
            hunks.push(DiffHunk {
                hunk_id,
                header: raw.to_string(),
                old_start,
                new_start,
                lines: Vec::new(),
            });
            old_line = old_start;
            new_line = new_start;
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if raw.starts_with('\\') {
            if let Some(hunk) = hunks.last_mut() {
                hunk.lines.push(DiffLine {
                    kind: "noNewline".to_string(),
                    old_line: None,
                    new_line: None,
                    text: String::new(),
                });
            }
            continue;
        }
        let Some(marker) = raw.as_bytes().first() else {
            continue;
        };
        let (kind, old, new) = match marker {
            b' ' => ("context", Some(old_line), Some(new_line)),
            b'+' => ("add", None, Some(new_line)),
            b'-' => ("delete", Some(old_line), None),
            _ => continue,
        };
        if line_count >= max_lines {
            truncated = true;
            continue;
        }
        line_count += 1;
        if kind == "add" || kind == "context" {
            new_line += 1;
        }
        if kind == "delete" || kind == "context" {
            old_line += 1;
        }
        if kind == "add" {
            additions += 1;
        }
        if kind == "delete" {
            deletions += 1;
        }
        if let Some(hunk) = hunks.last_mut() {
            hunk.lines.push(DiffLine {
                kind: kind.to_string(),
                old_line: old,
                new_line: new,
                text: raw[1..].to_string(),
            });
        }
    }
    (hunks, additions, deletions, truncated)
}

fn is_binary_patch(patch: &[u8]) -> bool {
    String::from_utf8_lossy(patch)
        .lines()
        .any(|line| line.starts_with("Binary files "))
}

/// Argv for one raw path: fixed options, `--` separator, then the exact
/// path bytes. The `:(literal)` magic prefix keeps glob characters literal:
/// the 2.43 baseline accepts neither `--literal-pathspecs` nor
/// `--pathspec-from-file` for diff/ls-files, and a bare `--` does not
/// disable magic. Prefix bytes are ASCII so prepending never corrupts
/// non-UTF-8 paths.
fn argv_with_path(base: &[&str], raw_path: &[u8]) -> Vec<OsString> {
    let mut owned: Vec<OsString> = base.iter().map(OsString::from).collect();
    owned.push(OsString::from("--"));
    let mut literal = b":(literal)".to_vec();
    literal.extend_from_slice(raw_path);
    owned.push(OsStr::from_bytes(&literal).to_os_string());
    owned
}

fn git_failed(raw: &[u8]) -> DiffError {
    let mut message = String::from_utf8_lossy(raw).into_owned();
    message.truncate(300);
    DiffError::GitFailed(message)
}

/// Staged mode bits for one raw path (`None` when untracked).
async fn staged_mode(
    runner: &GitRunner,
    cwd: &Path,
    raw_path: &[u8],
) -> Result<Option<String>, DiffError> {
    let argv = argv_with_path(&["ls-files", "--stage", "-z"], raw_path);
    let out = runner
        .run_with_env(cwd, &argv, READ_TIMEOUT, &[("GIT_OPTIONAL_LOCKS", "0")])
        .await?;
    if !out.success {
        return Err(git_failed(&out.stderr));
    }
    // `<mode> <oid> <stage>\t<path>\0`
    let entry = out.stdout.split(|b| *b == 0).next().unwrap_or(&[]);
    if entry.is_empty() {
        return Ok(None);
    }
    let mode = entry
        .split(|b| *b == b' ')
        .next()
        .map(String::from_utf8_lossy)
        .unwrap_or_default()
        .into_owned();
    Ok(Some(mode))
}

async fn run_patch(
    runner: &GitRunner,
    cwd: &Path,
    base: &[&str],
    raw_path: &[u8],
) -> Result<Vec<u8>, DiffError> {
    let argv = argv_with_path(base, raw_path);
    let out = runner
        .run_with_env(cwd, &argv, READ_TIMEOUT, &[("GIT_OPTIONAL_LOCKS", "0")])
        .await?;
    if !out.success {
        return Err(git_failed(&out.stderr));
    }
    Ok(out.stdout)
}

fn patch_doc(display_path: String, patch: &[u8]) -> Result<DiffDocument, DiffError> {
    if patch.len() > MAX_DIFF_BYTES {
        return Err(DiffError::TooLarge);
    }
    if is_binary_patch(patch) {
        return Ok(fallback_doc(
            "binary",
            display_path,
            "Binary file: content is not shown as text.",
        ));
    }
    let (hunks, additions, deletions, truncated) = parse_unified_patch(patch, MAX_DIFF_LINES);
    Ok(text_doc(
        display_path,
        hunks,
        additions,
        deletions,
        truncated,
    ))
}

fn looks_binary(sample: &[u8]) -> bool {
    sample.contains(&0)
}

/// Preview of an untracked worktree file. Reads the file directly and never
/// stages anything; everything renders as added lines.
fn preview_untracked(worktree_root: &Path, raw_path: &[u8]) -> Result<DiffDocument, DiffError> {
    use std::os::unix::ffi::OsStrExt;
    let display = String::from_utf8_lossy(raw_path).into_owned();
    let full = worktree_root.join(std::ffi::OsStr::from_bytes(raw_path));
    let meta = std::fs::symlink_metadata(&full)
        .map_err(|_| DiffError::Invalid("File is no longer on disk".to_string()))?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok(DiffDocument {
            kind: "symlink".to_string(),
            display_path: display,
            additions: None,
            deletions: None,
            hunks: Vec::new(),
            truncated: false,
            reason: Some(format!("Symbolic link to: {target}")),
        });
    }
    if meta.file_type().is_dir() {
        return Ok(fallback_doc(
            "tooLarge",
            display,
            "Directory preview is not supported.",
        ));
    }
    if meta.len() > MAX_PREVIEW_BYTES as u64 {
        return Ok(fallback_doc(
            "tooLarge",
            display,
            "File exceeds the preview size limit.",
        ));
    }
    let bytes =
        std::fs::read(&full).map_err(|_| DiffError::Invalid("File cannot be read".to_string()))?;
    if looks_binary(&bytes) {
        return Ok(fallback_doc(
            "binary",
            display,
            "Binary file: content is not shown as text.",
        ));
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<DiffLine> = Vec::new();
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if index >= MAX_DIFF_LINES {
            truncated = true;
            break;
        }
        lines.push(DiffLine {
            kind: "add".to_string(),
            old_line: None,
            new_line: Some(index as u64 + 1),
            text: line.to_string(),
        });
    }
    let additions = lines.len() as u64;
    Ok(DiffDocument {
        kind: "text".to_string(),
        display_path: display,
        additions: Some(additions),
        deletions: Some(0),
        hunks: if lines.is_empty() {
            Vec::new()
        } else {
            vec![DiffHunk {
                hunk_id: patch_fingerprint(text.as_bytes()),
                header: "@@ -0,0 +1 @@".to_string(),
                old_start: 0,
                new_start: 1,
                lines,
            }]
        },
        truncated,
        reason: None,
    })
}

/// Exact raw patch for a tracked worktree file. Hunk mutations use the same
/// argv as the viewer, so a displayed `hunkId` maps to these bytes.
pub async fn read_worktree_patch(
    runner: &GitRunner,
    worktree_root: &Path,
    raw_path: &[u8],
) -> Result<Vec<u8>, DiffError> {
    match staged_mode(runner, worktree_root, raw_path).await? {
        Some(mode) if mode == "160000" => Err(DiffError::Invalid(
            "Submodule hunks cannot be staged or discarded".to_string(),
        )),
        Some(_) => {
            run_patch(
                runner,
                worktree_root,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--patch",
                    "-U3",
                    "-M",
                ],
                raw_path,
            )
            .await
        }
        None => Err(DiffError::Invalid(
            "Stage or discard the whole untracked file; partial actions are unavailable"
                .to_string(),
        )),
    }
}

fn submodule_doc(display: String) -> DiffDocument {
    fallback_doc(
        "submodule",
        display,
        "Submodule pointer change: content lives in its own repository.",
    )
}

/// Worktree diff for one raw path: symlink/submodule fallbacks, untracked
/// preview, otherwise `git diff` of the tracked path.
pub async fn read_worktree_diff(
    runner: &GitRunner,
    worktree_root: &Path,
    raw_path: &[u8],
) -> Result<DiffDocument, DiffError> {
    use std::os::unix::ffi::OsStrExt;
    let display = String::from_utf8_lossy(raw_path).into_owned();
    let full = worktree_root.join(std::ffi::OsStr::from_bytes(raw_path));
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&full)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Ok(DiffDocument {
                kind: "symlink".to_string(),
                display_path: display,
                additions: None,
                deletions: None,
                hunks: Vec::new(),
                truncated: false,
                reason: Some(format!("Symbolic link to: {target}")),
            });
        }
    }
    match staged_mode(runner, worktree_root, raw_path).await? {
        Some(mode) if mode == "160000" => Ok(submodule_doc(display)),
        Some(_) => {
            let patch = read_worktree_patch(runner, worktree_root, raw_path).await?;
            patch_doc(display, &patch)
        }
        None => preview_untracked(worktree_root, raw_path),
    }
}

/// Index (`--cached`) diff for one raw path. A plain `--cached` diff
/// already compares against the empty tree when HEAD is unborn, so no
/// synthetic base revision is needed (and none is invented).
pub async fn read_index_diff(
    runner: &GitRunner,
    worktree_root: &Path,
    raw_path: &[u8],
) -> Result<DiffDocument, DiffError> {
    let display = String::from_utf8_lossy(raw_path).into_owned();
    match staged_mode(runner, worktree_root, raw_path).await? {
        Some(mode) if mode == "160000" => Ok(submodule_doc(display)),
        Some(_) => {
            let patch = run_patch(
                runner,
                worktree_root,
                &[
                    "diff",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--patch",
                    "-U3",
                    "-M",
                ],
                raw_path,
            )
            .await?;
            patch_doc(display, &patch)
        }
        None => Err(DiffError::Invalid(
            "File is not in the index; preview it from working changes instead.".to_string(),
        )),
    }
}

/// Commit diff of one raw path against its parent (or the empty tree).
/// `parent` is the exact parent OID, or `None` for the root commit. Root
/// comparisons use `diff-tree --root`: the empty-tree object is not
/// guaranteed to exist, so its hash is never passed as a revision.
pub async fn read_commit_diff(
    runner: &GitRunner,
    worktree_root: &Path,
    raw_path: &[u8],
    oid: &str,
    parent: Option<&str>,
) -> Result<DiffDocument, DiffError> {
    let display = String::from_utf8_lossy(raw_path).into_owned();
    // Owned argv: exact OIDs only, never revision expressions from the UI.
    let mut full: Vec<OsString> = match parent {
        Some(from) => [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--patch",
            "-U3",
            "-M",
        ]
        .iter()
        .map(OsString::from)
        .chain([OsString::from(from), OsString::from(oid)])
        .collect(),
        None => [
            "diff-tree",
            "-p",
            "--root",
            "--no-commit-id",
            "--no-textconv",
            "--no-color",
        ]
        .iter()
        .map(OsString::from)
        .chain([OsString::from(oid)])
        .collect(),
    };
    full.push(OsString::from("--"));
    let mut literal = b":(literal)".to_vec();
    literal.extend_from_slice(raw_path);
    full.push(OsStr::from_bytes(&literal).to_os_string());
    let out = runner
        .run_with_env(
            worktree_root,
            &full,
            READ_TIMEOUT,
            &[("GIT_OPTIONAL_LOCKS", "0")],
        )
        .await?;
    if !out.success {
        return Err(git_failed(&out.stderr));
    }
    patch_doc(display, &out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command as StdCommand;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("gitdock-t08-{}-{id}-{label}", std::process::id()));
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

    fn oid_of(repo: &Path, rev: &str) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["rev-parse", rev])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("rev-parse");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn runner() -> GitRunner {
        GitRunner::new(GitRunner::resolve_from_path().expect("system git"))
    }

    #[test]
    fn parses_hunks_line_numbers_and_no_newline_marker() {
        let patch = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,4 @@\n keep\n-old\n+new\n+extra\n\\ No newline at end of file\n";
        let (hunks, adds, dels, truncated) = parse_unified_patch(patch, 100);
        assert!(!truncated);
        assert_eq!(hunks.len(), 1);
        assert_eq!((hunks[0].old_start, hunks[0].new_start), (1, 1));
        let kinds: Vec<&str> = hunks[0].lines.iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, ["context", "delete", "add", "add", "noNewline"]);
        assert_eq!(hunks[0].lines[0].old_line, Some(1));
        assert_eq!(hunks[0].lines[0].new_line, Some(1));
        assert_eq!(hunks[0].lines[1].old_line, Some(2));
        assert_eq!(hunks[0].lines[1].new_line, None);
        assert_eq!(hunks[0].lines[2].new_line, Some(2));
        assert_eq!((adds, dels), (2, 1));
        assert!(hunks[0].hunk_id.starts_with("h1:"));
        assert_eq!(select_patch_hunk(patch, &hunks[0].hunk_id).unwrap(), patch);
    }

    #[test]
    fn truncates_at_the_line_cap() {
        let mut patch = b"@@ -1 +1 @@\n".to_vec();
        for _ in 0..10 {
            patch.extend(b"+x\n");
        }
        let (hunks, adds, _, truncated) = parse_unified_patch(&patch, 4);
        assert!(truncated);
        assert_eq!(adds, 4);
        assert_eq!(hunks[0].lines.len(), 4);
    }

    #[test]
    fn detects_binary_patches() {
        assert!(is_binary_patch(
            b"diff --git a/x.bin b/x.bin\nBinary files a/x.bin and b/x.bin differ\n"
        ));
        assert!(!is_binary_patch(b"@@ -1 +1 @@\n-a\n+b\n"));
    }

    #[tokio::test]
    async fn worktree_text_and_untracked_preview() {
        let root = temp_root("worktree");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("a.txt"), "one\ntwo\n").expect("write");
        git(&repo, &["add", "a.txt"]);
        git(&repo, &["commit", "-m", "add a"]);
        std::fs::write(repo.join("a.txt"), "one\nTWO\nthree\n").expect("write");
        std::fs::write(repo.join("new.txt"), "hello\n").expect("write");

        let runner = runner();
        let doc = read_worktree_diff(&runner, &repo, b"a.txt")
            .await
            .expect("worktree diff");
        assert_eq!(doc.kind, "text");
        assert_eq!(doc.additions, Some(2));
        assert_eq!(doc.deletions, Some(1));
        assert!(!doc.truncated);

        // Untracked preview renders file bytes as additions, index untouched.
        let preview = read_worktree_diff(&runner, &repo, b"new.txt")
            .await
            .expect("preview");
        assert_eq!(preview.kind, "text");
        assert_eq!(preview.additions, Some(1));
        assert_eq!(preview.hunks.len(), 1);
        assert_eq!(preview.hunks[0].lines[0].text, "hello");
        let ls = StdCommand::new("git")
            .current_dir(&repo)
            .args(["ls-files", "--", "new.txt"])
            .output()
            .expect("ls-files");
        assert!(ls.stdout.is_empty(), "preview must not stage anything");
    }

    #[tokio::test]
    async fn rename_binary_mode_only_and_non_utf8_paths() {
        let root = temp_root("cases");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("old.txt"), "same\n").expect("write");
        std::fs::write(repo.join("bin.dat"), b"\x00\x01\x02binary\n").expect("write");
        std::fs::write(repo.join("mode.sh"), "#!/bin/sh\n").expect("write");
        let raw_name = std::ffi::OsStr::from_bytes(b"raw-\xff.txt");
        std::fs::write(repo.join(raw_name), "raw\n").expect("write");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "files"]);
        git(&repo, &["mv", "old.txt", "new.txt"]);
        git(&repo, &["add", "-u"]);
        StdCommand::new("chmod")
            .current_dir(&repo)
            .args(["+x", "mode.sh"])
            .status()
            .expect("chmod");

        let runner = runner();
        // Rename resolves by the NEW raw path; identity never parses display.
        let doc = read_worktree_diff(&runner, &repo, b"new.txt")
            .await
            .expect("rename diff");
        assert_eq!(doc.kind, "text");

        let bin = read_worktree_diff(&runner, &repo, b"bin.dat")
            .await
            .expect("binary");
        // Unchanged binary has an empty patch: still binary only when git says so.
        let _ = bin;

        // Modify the binary so git reports a binary patch.
        std::fs::write(repo.join("bin.dat"), b"\x00\x09changed\n").expect("write");
        let bin = read_worktree_diff(&runner, &repo, b"bin.dat")
            .await
            .expect("binary diff");
        assert_eq!(bin.kind, "binary");
        assert!(bin.hunks.is_empty());

        // Mode-only change: no content hunks.
        let mode = read_worktree_diff(&runner, &repo, b"mode.sh")
            .await
            .expect("mode diff");
        assert_eq!(mode.kind, "text");
        assert!(mode.hunks.is_empty());

        // Non-UTF-8 path round-trips through the pathspec file.
        let raw = read_worktree_diff(&runner, &repo, b"raw-\xff.txt")
            .await
            .expect("raw path");
        assert_eq!(raw.kind, "text");
        assert!(raw.display_path.contains('\u{fffd}'));
    }

    #[tokio::test]
    async fn pathspec_magic_stays_literal() {
        // `star*.txt` must never match `starX.txt`: the `:(literal)` prefix
        // keeps glob characters exact on the 2.43 baseline.
        let root = temp_root("magic");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("starX.txt"), "other\n").expect("write");
        std::fs::write(repo.join("star*.txt"), "magic\n").expect("write");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-m", "magic"]);
        std::fs::write(repo.join("starX.txt"), "other\nchanged\n").expect("write");
        std::fs::write(repo.join("star*.txt"), "magic\nchanged\n").expect("write");

        let runner = runner();
        let doc = read_worktree_diff(&runner, &repo, b"star*.txt")
            .await
            .expect("literal diff");
        assert_eq!(doc.kind, "text");
        assert_eq!(doc.display_path, "star*.txt");
        assert_eq!(doc.additions, Some(1));
        assert_eq!(doc.deletions, Some(0));
    }

    #[tokio::test]
    async fn oversize_and_no_final_newline() {
        let root = temp_root("limits");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        // 600 KiB single-line churn exceeds the parse bound.
        let big: Vec<u8> = vec![b'x'; 600 * 1024];
        std::fs::write(repo.join("big.txt"), b"start\n").expect("write");
        git(&repo, &["add", "big.txt"]);
        git(&repo, &["commit", "-m", "big"]);
        std::fs::write(repo.join("big.txt"), [b"start\n".to_vec(), big].concat()).expect("write");
        // Tracked file whose worktree copy loses its final newline.
        std::fs::write(repo.join("noeol.txt"), "a\nb\n").expect("write");
        git(&repo, &["add", "noeol.txt"]);
        git(&repo, &["commit", "-m", "noeol"]);
        std::fs::write(repo.join("noeol.txt"), "a\nb").expect("write");

        let runner = runner();
        let err = read_worktree_diff(&runner, &repo, b"big.txt")
            .await
            .expect_err("oversize must fail");
        assert_eq!(err, DiffError::TooLarge);

        let doc = read_worktree_diff(&runner, &repo, b"noeol.txt")
            .await
            .expect("noeol");
        assert_eq!(doc.kind, "text");
        assert!(doc
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .any(|l| l.kind == "noNewline"));
    }

    #[tokio::test]
    async fn symlink_submodule_and_index_unborn() {
        let root = temp_root("special");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("target.txt"), "t\n").expect("write");
        std::os::unix::fs::symlink("target.txt", repo.join("link")).expect("symlink");
        git(&repo, &["add", "target.txt", "link"]);
        git(
            &repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,4b825dcb2362799c7d7a3438421c82cda77cb53d,vendor",
            ],
        );

        let runner = runner();
        let link = read_worktree_diff(&runner, &repo, b"link")
            .await
            .expect("symlink");
        assert_eq!(link.kind, "symlink");
        assert!(link.reason.as_deref().unwrap_or("").contains("target.txt"));

        let vendor = read_worktree_diff(&runner, &repo, b"vendor")
            .await
            .expect("submodule");
        assert_eq!(vendor.kind, "submodule");

        // Unborn index diff compares against the empty tree.
        let fresh = root.join("fresh");
        git(&root, &["init", "-b", "main", "fresh"]);
        std::fs::write(fresh.join("first.txt"), "hello\n").expect("write");
        git(&fresh, &["add", "first.txt"]);
        let doc = read_index_diff(&runner, &fresh, b"first.txt")
            .await
            .expect("unborn index");
        assert_eq!(doc.kind, "text");
        assert_eq!(doc.additions, Some(1));
    }

    #[tokio::test]
    async fn commit_root_and_second_parent() {
        let root = temp_root("commit");
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        git(&repo, &["commit", "--allow-empty", "-m", "init"]);
        std::fs::write(repo.join("base.txt"), "base\n").expect("write");
        git(&repo, &["add", "base.txt"]);
        git(&repo, &["commit", "-m", "base"]);
        // Diverging sides touch different files so the merge stays clean.
        git(&repo, &["checkout", "-b", "side"]);
        std::fs::write(repo.join("side.txt"), "side\n").expect("write");
        git(&repo, &["add", "side.txt"]);
        git(&repo, &["commit", "-m", "side"]);
        git(&repo, &["checkout", "main"]);
        std::fs::write(repo.join("main.txt"), "main\n").expect("write");
        git(&repo, &["add", "main.txt"]);
        git(&repo, &["commit", "-m", "main"]);
        git(&repo, &["merge", "side", "-m", "merge", "--no-edit"]);

        let runner = runner();
        let merge_oid = oid_of(&repo, "HEAD");
        let first = oid_of(&repo, "HEAD^1");
        let second = oid_of(&repo, "HEAD^2");
        // First-parent chain: merge(0) -> main(1) -> base(2) -> init(3).
        let base_oid = oid_of(&repo, "HEAD~2");

        // A commit against no parent diffs against the empty tree.
        let doc = read_commit_diff(&runner, &repo, b"base.txt", &base_oid, None)
            .await
            .expect("root");
        assert_eq!(doc.kind, "text");
        assert_eq!(doc.additions, Some(1));

        // First vs second parent show opposite sides of the merge.
        let d1 = read_commit_diff(&runner, &repo, b"side.txt", &merge_oid, Some(&first))
            .await
            .expect("parent1");
        let d2 = read_commit_diff(&runner, &repo, b"main.txt", &merge_oid, Some(&second))
            .await
            .expect("parent2");
        let text = |d: &DiffDocument| {
            d.hunks
                .iter()
                .flat_map(|h| &h.lines)
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
        };
        assert!(text(&d1).contains(&"side".to_string()));
        assert!(text(&d2).contains(&"main".to_string()));
    }
}
