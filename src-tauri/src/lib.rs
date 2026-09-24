//! GitDock Rust backend: typed Tauri commands over a Git CLI engine.
//!
//! Layout follows docs/03-architecture.md: `commands` are thin IPC handlers,
//! `domain` holds shared IDs/errors/DTOs. Git execution arrives in T03; T01
//! provides only the envelope types and the `app_preflight` read command.

pub mod commands;
pub mod domain;
pub mod git;
pub mod persistence;
pub mod services;

use commands::prelude::*;
use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(services::RepoRegistry::default()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_preflight,
            repo_open,
            repo_init,
            repo_close,
            repo_snapshot,
            repo_trust_set,
            repo_recent_list,
            repo_recent_remove,
            workspaces_save,
            workspaces_restore,
            repo_refs,
            history_page,
            history_search,
            commit_details,
            repo_status,
            operation_get,
            operation_cancel,
            diff_read,
            index_stage,
            index_unstage,
            diff_hunk_stage,
            diff_hunk_discard,
            worktree_discard_file,
            identity_read,
            commit_create,
            branch_create,
            branch_switch,
            branch_delete,
            branch_move,
            branch_rename,
            branch_set_upstream,
            branch_push,
            history_checkout,
            tag_create_at,
            history_push_to,
            cherry_pick_onto,
            revert_commit,
            merge_commit,
            rebase_onto,
            reword_message,
            modify_commit,
            edit_author,
            split_commit,
            move_to_branch,
            rebase_interactive_from,
            reset_soft,
            reset_mixed,
            reset_hard,
            confirmation_prepare,
            bitbucket_connect,
            remote_status,
            remote_fetch,
            remote_pull,
            remote_push,
            operation_log,
            stash_list,
            stash_save,
            stash_apply,
            conflict_list,
            conflict_preview,
            conflict_accept,
            conflict_mark_resolved,
            merge_start,
            merge_complete,
            merge_abort,
            settings_get,
            settings_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running GitDock");
}
