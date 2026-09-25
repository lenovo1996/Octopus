//! History read commands: refs, paged history, search, commit details.
//!
//! Pagination cursors are opaque `{session}:{offset}` strings over a pinned
//! topology snapshot, so pages never duplicate or skip rows while refs move.
//! Every oid from the UI is validated as hex of the session's object format;
//! UI oids are never interpolated into revision expressions.

use std::collections::HashMap;

use serde::Deserialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::domain::{
    ApiResult, AppError, CommitDetails, CommitFileChange, CommitRow, ErrorCode, HistoryPage,
    RecoveryAction, RefItem, RequestId, SearchResults,
};
use crate::git::{
    list_refs, read_commit_files, read_metadata, read_topology, CommitMeta, GitRunner, TopoRow,
    READ_TIMEOUT,
};
use crate::services::{HistorySession, RepoRegistry, RepoSession};

const MAX_LIMIT: usize = 200;

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

fn check_limit(limit: usize) -> Result<usize, AppError> {
    if limit == 0 || limit > MAX_LIMIT {
        return Err(bad_request("limit must be 1..=200"));
    }
    Ok(limit)
}

/// Hex oid of the session's object format (sha1: 40, sha256: 64).
fn check_oid(session: &RepoSession, oid: &str) -> Result<(), AppError> {
    let expected = if session.object_format == "sha256" {
        64
    } else {
        40
    };
    if oid.len() == expected && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(bad_request("Malformed object id"))
    }
}

fn parse_cursor(cursor: &str) -> Result<(String, usize), AppError> {
    let (id, offset) = cursor
        .rsplit_once(':')
        .ok_or_else(|| bad_request("Invalid page cursor"))?;
    if id.is_empty() {
        return Err(bad_request("Invalid page cursor"));
    }
    let offset: usize = offset
        .parse()
        .map_err(|_| bad_request("Invalid page cursor"))?;
    Ok((id.to_string(), offset))
}

fn cursor_for(session_id: &str, offset: usize) -> String {
    format!("{session_id}:{offset}")
}

// ---- Scope resolution ----

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryScope {
    #[serde(rename = "type")]
    pub scope_type: String,
    pub ref_id: Option<String>,
}

async fn resolve_tips(
    runner: &GitRunner,
    session: &RepoSession,
    scope: &HistoryScope,
) -> Result<Vec<String>, AppError> {
    match scope.scope_type.as_str() {
        "allRefs" => {
            let refs = list_refs(runner, session).await?;
            let mut tips: Vec<String> = refs.into_iter().map(|r| r.oid).collect();
            if let Ok(head) = runner
                .run(&session.worktree_root, &["rev-parse", "HEAD"], READ_TIMEOUT)
                .await
            {
                if head.success {
                    let oid = String::from_utf8_lossy(&head.stdout).trim().to_string();
                    if !oid.is_empty() && !tips.contains(&oid) {
                        tips.push(oid);
                    }
                }
            }
            Ok(tips)
        }
        "head" => {
            let out = runner
                .run(&session.worktree_root, &["rev-parse", "HEAD"], READ_TIMEOUT)
                .await
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::GIT_ERROR,
                        "Failed to resolve HEAD",
                        RecoveryAction::Refresh,
                        true,
                    )
                })?;
            if out.success {
                let oid = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if oid.is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(vec![oid])
                }
            } else {
                // Unborn HEAD: valid state, empty history.
                Ok(Vec::new())
            }
        }
        "ref" => {
            let wanted = scope
                .ref_id
                .as_deref()
                .ok_or_else(|| bad_request("ref scope requires refId"))?;
            let refs = list_refs(runner, session).await?;
            refs.into_iter()
                .find(|r| r.ref_id == wanted)
                .map(|r| vec![r.oid])
                .ok_or_else(|| {
                    AppError::new(
                        ErrorCode::REF_INVALID,
                        "Unknown ref",
                        RecoveryAction::Refresh,
                        true,
                    )
                })
        }
        _ => Err(bad_request("Unknown history scope")),
    }
}

fn build_refmap(refs: &[RefItem]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for r in refs {
        map.entry(r.oid.clone()).or_default().push(r.ref_id.clone());
    }
    map
}

async fn ensure_session(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    session: &RepoSession,
    scope: &HistoryScope,
    cursor: Option<&str>,
) -> Result<(HistorySession, usize), AppError> {
    if let Some(cursor) = cursor {
        let (id, offset) = parse_cursor(cursor)?;
        let cached = registry.history_get(&id).ok_or_else(|| {
            AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "Unknown or expired history session; query again",
                RecoveryAction::Refresh,
                false,
            )
        })?;
        if cached.repo_id != session.repo_id {
            return Err(AppError::new(
                ErrorCode::INVALID_ARGUMENT,
                "History cursor belongs to another repository",
                RecoveryAction::Refresh,
                false,
            ));
        }
        if offset > cached.topo.len() {
            return Err(bad_request("Page cursor is out of range"));
        }
        return Ok((cached.clone(), offset));
    }
    let tips = resolve_tips(runner, session, scope).await?;
    let refs = list_refs(runner, session).await?;
    let refmap = build_refmap(&refs);
    let (topo, truncated) = read_topology(runner, session, &tips).await?;
    Ok((
        registry.history_put(&session.repo_id, tips, topo, refmap, truncated),
        0,
    ))
}

fn row_for(
    topo: &TopoRow,
    meta_by_oid: &HashMap<String, CommitMeta>,
    refmap: &HashMap<String, Vec<String>>,
) -> Option<CommitRow> {
    let meta = meta_by_oid.get(&topo.oid)?;
    Some(CommitRow {
        oid: topo.oid.clone(),
        parents: topo.parents.clone(),
        subject: meta.subject.clone(),
        author_name: meta.author_name.clone(),
        authored_at: meta.authored_at.clone(),
        committed_at: meta.committed_at.clone(),
        refs: refmap.get(&topo.oid).cloned().unwrap_or_default(),
        boundary: topo.boundary,
    })
}

async fn core_page(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    scope: &HistoryScope,
    cursor: Option<&str>,
    limit: usize,
) -> Result<HistoryPage, AppError> {
    let limit = check_limit(limit)?;
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    let (history, offset) = ensure_session(runner, registry, &session, scope, cursor).await?;
    let end = (offset + limit).min(history.topo.len());
    let slice: Vec<String> = history.topo[offset..end]
        .iter()
        .map(|r| r.oid.clone())
        .collect();
    let metas = read_metadata(runner, &session, &slice).await?;
    let meta_by_oid: HashMap<String, CommitMeta> =
        metas.into_iter().map(|m| (m.oid.clone(), m)).collect();
    let rows: Vec<CommitRow> = history.topo[offset..end]
        .iter()
        .filter_map(|t| row_for(t, &meta_by_oid, &history.refmap))
        .collect();
    Ok(HistoryPage {
        history_session_id: history.id.clone(),
        next_cursor: if end < history.topo.len() {
            Some(cursor_for(&history.id, end))
        } else {
            None
        },
        // Only the final page may claim truncation: earlier pages still
        // have cached rows ahead, so the flag would be a lie there.
        truncated: history.truncated && end >= history.topo.len(),
        rows,
    })
}

async fn core_search(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    scope: &HistoryScope,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<SearchResults, AppError> {
    let limit = check_limit(limit)?;
    if query.is_empty() || query.len() > 500 {
        return Err(bad_request("Search query must be 1..=500 characters"));
    }
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    // Fresh scan per query; cursor only carries the offset into matches.
    // Refetching keeps results literal and simple; UI discards stale pages.
    let offset = match cursor {
        Some(c) => {
            let (_, offset) = parse_cursor(c)?;
            offset
        }
        None => 0,
    };
    let tips = resolve_tips(runner, &session, scope).await?;
    let refs = list_refs(runner, &session).await?;
    let refmap = build_refmap(&refs);
    let (topo, truncated) = read_topology(runner, &session, &tips).await?;
    let all_oids: Vec<String> = topo.iter().map(|r| r.oid.clone()).collect();
    let metas = read_metadata(runner, &session, &all_oids).await?;
    let meta_by_oid: HashMap<String, CommitMeta> =
        metas.into_iter().map(|m| (m.oid.clone(), m)).collect();
    let needle = query.to_lowercase();
    let matched: Vec<&TopoRow> = topo
        .iter()
        .filter(|t| {
            meta_by_oid.get(&t.oid).is_some_and(|m| {
                m.subject.to_lowercase().contains(&needle)
                    || m.author_name.to_lowercase().contains(&needle)
                    || t.oid.to_lowercase().starts_with(&needle)
            })
        })
        .collect();
    if offset > matched.len() {
        return Err(bad_request("Page cursor is out of range"));
    }
    let end = (offset + limit).min(matched.len());
    let rows: Vec<CommitRow> = matched[offset..end]
        .iter()
        .filter_map(|t| row_for(t, &meta_by_oid, &refmap))
        .collect();
    // Search pagination re-scans, so the cursor is offset-only and validated
    // against the current match count, never a stored session. On a
    // truncated walk the scan covers cached (newest) commits only.
    let search_id = "search";
    Ok(SearchResults {
        rows,
        next_cursor: if end < matched.len() {
            Some(cursor_for(search_id, end))
        } else {
            None
        },
        incomplete: truncated,
    })
}

pub(crate) async fn core_details(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
    oid: &str,
    parent_index: Option<usize>,
) -> Result<CommitDetails, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    check_oid(&session, oid)?;
    let metas = read_metadata(runner, &session, &[oid.to_string()]).await?;
    let meta = metas.into_iter().next().ok_or_else(|| {
        AppError::new(
            ErrorCode::REF_INVALID,
            "Unknown commit",
            RecoveryAction::Refresh,
            true,
        )
    })?;
    let (used_parent, files) =
        read_commit_files(runner, &session, oid, parent_index, &meta.parents).await?;
    // Cache raw (new, old) paths per commit + parent so `diff_read` resolves
    // commit file tokens without ever parsing display strings (contract §3).
    let repo_id = session.repo_id.clone();
    let oid_owned = meta.oid.clone();
    registry.commit_files_put(
        &repo_id,
        &oid_owned,
        used_parent,
        files.iter().map(|f| f.raw_path.clone()).collect(),
        files.iter().map(|f| f.raw_old_path.clone()).collect(),
    );
    Ok(CommitDetails {
        oid: meta.oid,
        subject: meta.subject,
        body: meta.body,
        author_name: meta.author_name,
        authored_at: meta.authored_at,
        committed_at: meta.committed_at,
        parents: meta.parents,
        parent_index: used_parent,
        files: files
            .into_iter()
            .enumerate()
            .map(|(index, f)| CommitFileChange {
                status: f.status,
                path: f.path,
                old_path: f.old_path,
                path_id: registry
                    .commit_path_id(&repo_id, &oid_owned, used_parent, index)
                    .unwrap_or_default(),
            })
            .collect(),
    })
}

async fn core_refs(
    runner: &GitRunner,
    registry: &mut RepoRegistry,
    repo_id: &str,
) -> Result<Vec<RefItem>, AppError> {
    let session = registry.get(repo_id).ok_or_else(session_missing)?.clone();
    list_refs(runner, &session).await
}

// ---- IPC types ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoContext {
    pub request_id: RequestId,
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPageRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub scope: HistoryScope,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySearchRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub scope: HistoryScope,
    pub query: String,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetailsRequest {
    pub request_id: RequestId,
    pub repo_id: String,
    pub oid: String,
    pub parent_index: Option<usize>,
}

// ---- Commands ----

#[tauri::command]
pub async fn repo_refs(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: RepoContext,
) -> Result<ApiResult<Vec<RefItem>>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_refs(&runner, &mut registry, &request.repo_id).await
    }
    .await;
    Ok(match result {
        Ok(data) => ApiResult::ok(data, request_id),
        Err(error) => ApiResult::err(error, request_id),
    })
}

#[tauri::command]
pub async fn history_page(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistoryPageRequest,
) -> Result<ApiResult<HistoryPage>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_page(
            &runner,
            &mut registry,
            &request.repo_id,
            &request.scope,
            request.cursor.as_deref(),
            request.limit,
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
pub async fn history_search(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: HistorySearchRequest,
) -> Result<ApiResult<SearchResults>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_search(
            &runner,
            &mut registry,
            &request.repo_id,
            &request.scope,
            &request.query,
            request.cursor.as_deref(),
            request.limit,
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
pub async fn commit_details(
    registry: State<'_, Mutex<RepoRegistry>>,
    request: CommitDetailsRequest,
) -> Result<ApiResult<CommitDetails>, String> {
    let request_id = request.request_id.clone();
    let result = async {
        check_request_id(&request_id)?;
        let runner = git_runner()?;
        let mut registry = registry.lock().await;
        core_details(
            &runner,
            &mut registry,
            &request.repo_id,
            &request.oid,
            request.parent_index,
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
    pub use super::{commit_details, history_page, history_search, repo_refs};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Store;
    use crate::services::RepoRegistry;
    use std::path::{Path, PathBuf};
    use std::process::Command as StdCommand;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(1000);

    fn temp_root(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("gitdock-t04-{}-{}-{label}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn git(cwd: &Path, args: &[&str]) -> String {
        let output = StdCommand::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Octopus Test")
            .env("GIT_AUTHOR_EMAIL", "octopus-test@example.com")
            .env("GIT_COMMITTER_NAME", "Octopus Test")
            .env("GIT_COMMITTER_EMAIL", "octopus-test@example.com")
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed in {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn commit_empty(repo: &Path, message: &str, day: u32) {
        let date = format!("2026-01-{day:02}T10:00:00+07:00");
        let status = StdCommand::new("git")
            .current_dir(repo)
            .args(["commit", "--allow-empty", "-m", message])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@x")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@x")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .status()
            .expect("spawn git commit");
        assert!(status.success());
    }

    fn commit_file(repo: &Path, name: &str, content: &str, message: &str) {
        std::fs::write(repo.join(name), content).expect("write");
        git(repo, &["add", name]);
        let status = StdCommand::new("git")
            .current_dir(repo)
            .args(["commit", "-m", message])
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@x")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@x")
            .status()
            .expect("spawn git commit");
        assert!(status.success());
    }

    async fn open_repo(
        runner: &GitRunner,
        registry: &mut RepoRegistry,
        store: &mut Store,
        path: &Path,
    ) -> String {
        let discovered = crate::git::discover(runner, path).await.expect("discover");
        let session = registry.open(&discovered, crate::domain::TrustState::ReadOnly);
        store.push_recent(&session.key(), &session.display_path);
        session.repo_id.clone()
    }

    fn harness() -> (GitRunner, RepoRegistry, Store, PathBuf) {
        let root = temp_root("harness");
        let store_dir = root.join("store");
        std::fs::create_dir_all(&store_dir).expect("store dir");
        let exe = GitRunner::resolve_from_path().expect("system git");
        (
            GitRunner::new(exe),
            RepoRegistry::default(),
            Store::open(&store_dir),
            root,
        )
    }

    fn scope_all() -> HistoryScope {
        HistoryScope {
            scope_type: "allRefs".to_string(),
            ref_id: None,
        }
    }

    #[tokio::test]
    async fn refs_peel_and_current() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "first");
        git(&repo, &["tag", "light"]);
        git(&repo, &["tag", "-a", "annot", "-m", "release"]);
        git(&repo, &["branch", "feature"]);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;

        let refs = core_refs(&runner, &mut registry, &repo_id)
            .await
            .expect("refs");
        let by_name: HashMap<&str, &RefItem> =
            refs.iter().map(|r| (r.full_name.as_str(), r)).collect();
        let main = by_name["refs/heads/main"];
        assert!(main.current);
        assert_eq!(main.kind, "local");
        // Annotated tag peels to the commit, lightweight points at it directly.
        assert_eq!(by_name["refs/tags/annot"].oid, main.oid);
        assert_eq!(by_name["refs/tags/annot"].kind, "tag");
        assert_eq!(by_name["refs/tags/light"].oid, main.oid);
        assert!(!by_name["refs/heads/feature"].current);
    }

    #[tokio::test]
    async fn pagination_matches_raw_git_oracle() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        for i in 0..65 {
            commit_empty(&repo, &format!("commit number {i}"), 1 + (i % 27) as u32);
        }
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;

        // Oracle straight from git: topo order with parents.
        let oracle = git(&repo, &["rev-list", "--topo-order", "--parents", "HEAD"]);
        let expected: Vec<(String, Vec<String>)> = oracle
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut parts = l.split_whitespace();
                (
                    parts.next().unwrap().to_string(),
                    parts.map(str::to_string).collect(),
                )
            })
            .collect();
        assert_eq!(expected.len(), 65);

        let mut oids = Vec::new();
        let mut parents_seen: HashMap<String, Vec<String>> = HashMap::new();
        let mut cursor: Option<String> = None;
        let mut session_id = String::new();
        loop {
            let page = core_page(
                &runner,
                &mut registry,
                &repo_id,
                &scope_all(),
                cursor.as_deref(),
                25,
            )
            .await
            .expect("page");
            if session_id.is_empty() {
                session_id = page.history_session_id.clone();
            }
            assert_eq!(page.history_session_id, session_id, "stable session");
            for row in &page.rows {
                assert!(!oids.contains(&row.oid), "no duplicates");
                oids.push(row.oid.clone());
                parents_seen.insert(row.oid.clone(), row.parents.clone());
            }
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(oids.len(), 65, "no skipped rows");
        for (i, (oid, parents)) in expected.iter().enumerate() {
            assert_eq!(&oids[i], oid, "topo order row {i}");
            assert_eq!(&parents_seen[oid], parents, "parents row {i}");
        }
    }

    #[tokio::test]
    async fn topology_truncates_at_explicit_cap() {
        // Regression: a monorepo walk longer than the runner output cap
        // must be cut with a flag, never fail the whole History panel.
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        for i in 0..8 {
            commit_empty(&repo, &format!("commit number {i}"), 1 + i as u32);
        }
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let session = registry.get(&repo_id).expect("session").clone();
        let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let (rows, truncated) =
            crate::git::read_topology_capped(&runner, &session, std::slice::from_ref(&head), 5)
                .await
                .expect("topo");
        assert!(truncated, "walk longer than cap must flag");
        assert_eq!(rows.len(), 5, "newest rows only");
        assert_eq!(rows[0].oid, head, "tip stays first");

        let (rows, truncated) = crate::git::read_topology_capped(&runner, &session, &[head], 64)
            .await
            .expect("topo");
        assert!(!truncated, "short walk is complete");
        assert_eq!(rows.len(), 8);
    }

    #[tokio::test]
    async fn metadata_batches_past_chunk_size() {
        // 2500 oids cross the 2000-per-batch split; duplicates of one
        // commit keep the fixture instant while proving multi-batch order.
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "only commit");
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let session = registry.get(&repo_id).expect("session").clone();
        let oid = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let oids = vec![oid.clone(); 2500];
        let metas = crate::git::read_metadata(&runner, &session, &oids)
            .await
            .expect("metas");
        assert_eq!(metas.len(), 2500);
        assert!(metas.iter().all(|m| m.oid == oid));
        assert_eq!(metas[0].subject, "only commit");
    }

    #[tokio::test]
    async fn page_flags_truncation_only_at_cache_end() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        for i in 0..8 {
            commit_empty(&repo, &format!("commit number {i}"), 1 + i as u32);
        }
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let session = registry.get(&repo_id).expect("session").clone();
        let head = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        let (full, truncated) =
            crate::git::read_topology_capped(&runner, &session, std::slice::from_ref(&head), 64)
                .await
                .expect("topo");
        assert!(!truncated);
        assert_eq!(full.len(), 8);
        // Simulate a capped walk: cache holds the newest 5 of 8.
        let cached = registry.history_put(
            &repo_id,
            vec![head],
            full[..5].to_vec(),
            HashMap::new(),
            true,
        );

        let scope = scope_all();
        let mid_cursor = cursor_for(&cached.id, 3);
        let mid = core_page(
            &runner,
            &mut registry,
            &repo_id,
            &scope,
            Some(&mid_cursor),
            1,
        )
        .await
        .expect("mid page");
        assert_eq!(mid.rows.len(), 1);
        assert!(mid.next_cursor.is_some(), "cache still has rows");
        assert!(!mid.truncated, "mid-cache page must not claim truncation");

        let end_cursor = cursor_for(&cached.id, 3);
        let last = core_page(
            &runner,
            &mut registry,
            &repo_id,
            &scope,
            Some(&end_cursor),
            10,
        )
        .await
        .expect("last page");
        assert_eq!(last.rows.len(), 2);
        assert!(last.next_cursor.is_none());
        assert!(last.truncated, "final page of a cut walk flags it");
    }

    #[tokio::test]
    async fn merge_details_and_parent_selection() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "base.txt", "base\n", "base");
        git(&repo, &["checkout", "-b", "feature"]);
        commit_file(&repo, "feat.txt", "feat\n", "feature work");
        git(&repo, &["checkout", "main"]);
        commit_file(&repo, "main.txt", "main\n", "main work");
        git(&repo, &["merge", "--no-ff", "-m", "merge it", "feature"]);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;

        let merge_oid = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        // Spec contract: parent defaults to 0 for every non-root commit,
        // merges included — refusing strands the details panel with no
        // way to pick a parent.
        let defaulted = core_details(&runner, &mut registry, &repo_id, &merge_oid, None)
            .await
            .expect("merge defaults to first parent");
        assert_eq!(defaulted.parents.len(), 2);
        assert_eq!(defaulted.parent_index, Some(0));

        let first = core_details(&runner, &mut registry, &repo_id, &merge_oid, Some(0))
            .await
            .expect("first parent diff");
        let second = core_details(&runner, &mut registry, &repo_id, &merge_oid, Some(1))
            .await
            .expect("second parent diff");
        assert_eq!(first.parents.len(), 2);
        assert_eq!(first.parent_index, Some(0));
        assert_eq!(second.parent_index, Some(1));
        let paths0: Vec<&str> = first.files.iter().map(|f| f.path.as_str()).collect();
        let paths1: Vec<&str> = second.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths0.contains(&"feat.txt"),
            "vs first parent shows feature side"
        );
        assert!(
            paths1.contains(&"main.txt"),
            "vs second parent shows main side"
        );

        let err = core_details(&runner, &mut registry, &repo_id, &merge_oid, Some(2))
            .await
            .expect_err("parent index out of range");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn tricky_message_roundtrip_against_git_oracle() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        let message = "fix (login) [urgent]: handle --weird \"quotes\"\n\nBody with\ttab, unicode: tieng viet, and --- dashes.\nSecond body line.\n";
        commit_file(&repo, "a.txt", "a\n", message);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let oid = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let details = core_details(&runner, &mut registry, &repo_id, &oid, None)
            .await
            .expect("details");
        let oracle = git(&repo, &["log", "-1", "--format=%B", &oid]);
        assert_eq!(
            format!("{}\n\n{}", details.subject, details.body),
            oracle.trim_end().to_string()
        );
        assert!(details.subject.contains("fix (login)"));
    }

    #[tokio::test]
    async fn root_and_invalid_oid_rules() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "root");
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let oid = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let root_details = core_details(&runner, &mut registry, &repo_id, &oid, None)
            .await
            .expect("root details");
        assert!(root_details.parents.is_empty());
        assert_eq!(root_details.parent_index, None);
        assert_eq!(root_details.files.len(), 1);

        let err = core_details(&runner, &mut registry, &repo_id, &oid, Some(0))
            .await
            .expect_err("root has no parent");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        let err = core_details(&runner, &mut registry, &repo_id, "xyz", None)
            .await
            .expect_err("malformed oid");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);

        let err = core_details(&runner, &mut registry, &repo_id, &"0".repeat(40), None)
            .await
            .expect_err("unknown oid");
        assert_eq!(err.code, ErrorCode::REF_INVALID);

        let err = core_page(&runner, &mut registry, &repo_id, &scope_all(), None, 0)
            .await
            .expect_err("limit 0");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
        let err = core_page(&runner, &mut registry, &repo_id, &scope_all(), None, 201)
            .await
            .expect_err("limit 201");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
        let err = core_page(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            Some("nope"),
            10,
        )
        .await
        .expect_err("bad cursor");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn search_is_literal_and_paginates() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "plain work");
        commit_file(&repo, "b.txt", "b\n", "fix (login) [urgent]");
        commit_file(&repo, "c.txt", "c\n", "more plain work");
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;

        // Parentheses/brackets are regex metacharacters; search must be literal.
        let results = core_search(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            "fix (login)",
            None,
            10,
        )
        .await
        .expect("search");
        assert_eq!(results.rows.len(), 1);
        assert!(results.rows[0].subject.contains("fix (login)"));
        assert!(!results.incomplete);

        // Case-insensitive literal.
        let results = core_search(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            "FIX (LOGIN)",
            None,
            10,
        )
        .await
        .expect("search");
        assert_eq!(results.rows.len(), 1);

        // Oid prefix.
        let oid = results.rows[0].oid.clone();
        let results = core_search(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            &oid[..8],
            None,
            10,
        )
        .await
        .expect("search");
        assert!(results.rows.iter().any(|r| r.oid == oid));

        // Pagination over matches, limit 1.
        let first = core_search(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            "work",
            None,
            1,
        )
        .await
        .expect("page 1");
        assert_eq!(first.rows.len(), 1);
        let cursor = first.next_cursor.expect("has next");
        let second = core_search(
            &runner,
            &mut registry,
            &repo_id,
            &scope_all(),
            "work",
            Some(&cursor),
            1,
        )
        .await
        .expect("page 2");
        assert_eq!(second.rows.len(), 1);
        assert_ne!(first.rows[0].oid, second.rows[0].oid);

        let err = core_search(&runner, &mut registry, &repo_id, &scope_all(), "", None, 10)
            .await
            .expect_err("empty query");
        assert_eq!(err.code, ErrorCode::INVALID_ARGUMENT);
    }

    #[tokio::test]
    async fn detached_and_unborn_scopes() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "one");
        git(&repo, &["checkout", "--detach", "HEAD"]);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;
        let head_scope = HistoryScope {
            scope_type: "head".to_string(),
            ref_id: None,
        };
        let page = core_page(&runner, &mut registry, &repo_id, &head_scope, None, 10)
            .await
            .expect("detached head page");
        assert_eq!(page.rows.len(), 1);

        let fresh = root.join("fresh");
        git(&root, &["init", "-b", "main", "fresh"]);
        let fresh_id = open_repo(&runner, &mut registry, &mut store, &fresh).await;
        let page = core_page(&runner, &mut registry, &fresh_id, &head_scope, None, 10)
            .await
            .expect("unborn page");
        assert!(page.rows.is_empty());
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn ref_scope_and_checked_out_elsewhere() {
        let (runner, mut registry, mut store, root) = harness();
        let repo = root.join("repo");
        git(&root, &["init", "-b", "main", "repo"]);
        commit_file(&repo, "a.txt", "a\n", "one");
        git(&repo, &["branch", "feature"]);
        git(&repo, &["worktree", "add", "../linked", "feature"]);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &repo).await;

        let refs = core_refs(&runner, &mut registry, &repo_id)
            .await
            .expect("refs");
        let feature = refs
            .iter()
            .find(|r| r.full_name == "refs/heads/feature")
            .expect("feature ref");
        assert!(feature.checked_out_elsewhere);

        let scope = HistoryScope {
            scope_type: "ref".to_string(),
            ref_id: Some("refs/heads/feature".to_string()),
        };
        let page = core_page(&runner, &mut registry, &repo_id, &scope, None, 10)
            .await
            .expect("ref page");
        assert_eq!(page.rows.len(), 1);

        let bad_scope = HistoryScope {
            scope_type: "ref".to_string(),
            ref_id: Some("refs/heads/nope".to_string()),
        };
        let err = core_page(&runner, &mut registry, &repo_id, &bad_scope, None, 10)
            .await
            .expect_err("unknown ref");
        assert_eq!(err.code, ErrorCode::REF_INVALID);
    }

    #[tokio::test]
    async fn shallow_clone_marks_boundary() {
        let (runner, mut registry, mut store, root) = harness();
        let src = root.join("src");
        git(&root, &["init", "-b", "main", "src"]);
        for i in 0..5 {
            commit_empty(&src, &format!("c{i}"), 1 + i as u32);
        }
        let url = format!("file://{}", src.display());
        git(&root, &["clone", "--depth", "2", &url, "shallow"]);
        let repo_id = open_repo(&runner, &mut registry, &mut store, &root.join("shallow")).await;
        let page = core_page(&runner, &mut registry, &repo_id, &scope_all(), None, 10)
            .await
            .expect("shallow page");
        assert_eq!(page.rows.len(), 2);
        assert!(!page.rows[0].boundary, "tip is not a boundary");
        assert!(page.rows[1].boundary, "grafted commit is a boundary");
    }
}
