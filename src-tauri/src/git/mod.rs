pub mod commit;
pub mod diff;
pub mod discover;
pub mod history;
pub mod remote;
pub mod runner;
pub mod status;

pub use commit::{
    commit_staged, has_unmerged, index_is_empty, read_identity, CommitError, Identity,
};
pub use diff::{
    empty_tree_hash, patch_fingerprint, read_commit_diff, read_index_diff, read_index_patch,
    read_worktree_diff, read_worktree_patch, select_patch_hunk, select_patch_lines, DiffError,
    MAX_DIFF_BYTES, MAX_DIFF_LINES, MAX_PREVIEW_BYTES,
};
pub use discover::{discover, validate_branch_name, DiscoveredRepo};
pub use history::{
    list_refs, read_commit_files, read_metadata, read_topology, read_topology_capped, ChangedPath,
    CommitMeta, TopoRow, MAX_TOPO_ROWS,
};
pub use remote::{
    ahead_behind, classify_network_stderr, parse_progress_line, redact_url, resolve_upstream,
    validate_remote_url, NetworkFault, RemoteKind, UpstreamRef, UrlError,
};
pub use runner::{GitRunner, RunError, NETWORK_TIMEOUT, READ_TIMEOUT, WRITE_TIMEOUT};
pub use status::{
    display_path, read_status, to_display_rows, BranchInfo, FileKind, HeadRef, ParsedStatus,
    StatusCounts, StatusError, StatusFile,
};
