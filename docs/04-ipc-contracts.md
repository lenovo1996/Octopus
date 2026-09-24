# IPC contracts — v1

Đây là hợp đồng thiết kế cần hiện thực tại T01–T14, chưa phải API có sẵn. Command dùng `snake_case`; JSON fields và enum discriminant dùng `camelCase`. Rust DTO là nguồn canonical, TypeScript generate từ DTO với công cụ đã pin hoặc kiểm tra schema parity trong CI; không duy trì hai bản khác nhau bằng tay.

## 1. Envelope và kiểu nền

```ts
type RepoId = string;           // Opaque, chỉ sống trong app session
type RequestId = string;        // UUID do client sinh
type OperationId = string;      // Opaque, backend sinh
type Oid = string;              // Full object id; không giả định luôn 40 chars
type Version = number;          // Safe integer, tăng đơn điệu trong repo session
type PathId = string;           // Opaque token backend map tới nguyên byte path
type RefId = string;            // Opaque token map tới fully-qualified ref

type ApiResult<T> =
  | { ok: true; data: T; requestId: RequestId }
  | { ok: false; error: AppError; requestId: RequestId };

interface AppError {
  code: ErrorCode;
  message: string;              // Plain text, user-safe
  recovery: "refresh" | "retryRead" | "configureGit" | "authenticate"
    | "resolveConflict" | "chooseRepository" | "inspectState" | "none";
  retryable: boolean;            // Không cho phép auto-retry mutation
  operationId?: OperationId;
  diagnosticId?: string;        // Tra log local đã redact
}

type ErrorCode = "GIT_NOT_FOUND" | "GIT_VERSION_UNSUPPORTED" | "NOT_REPOSITORY"
  | "BARE_REPOSITORY" | "REPO_UNAVAILABLE" | "INVALID_ARGUMENT" | "PATH_INVALID"
  | "REF_INVALID" | "TRUST_REQUIRED" | "REPO_BUSY" | "REPO_LOCKED"
  | "STALE_STATE" | "DIRTY_WORKTREE" | "EMPTY_INDEX" | "IDENTITY_MISSING"
  | "CONFLICTS_PRESENT" | "DIVERGED" | "AUTH_REQUIRED" | "NETWORK_ERROR"
  | "HOOK_FAILED" | "SIGNING_FAILED" | "OUTPUT_LIMIT" | "UNSUPPORTED"
  | "CANCELLED" | "TIMEOUT" | "IO_ERROR" | "GIT_ERROR";

interface ReadContext { requestId: RequestId; repoId: RepoId }
interface WriteContext extends ReadContext { expectedVersion: Version }
interface Versioned<T> { repoId: RepoId; version: Version; value: T }
```

Wrapper gọi `invoke<ApiResult<T>>(command, { request })`; Tauri transport reject (window đóng, deserialize lỗi) được normalize riêng, không giả thành Git failure. Validation runtime ở Rust bắt buộc. Opaque token không phải authorization: mọi command còn kiểm repo session, window và operation scope.

## 2. Snapshot và file identity

```ts
type HeadState =
  | { kind: "branch"; refId: RefId; name: string; oid: Oid }
  | { kind: "detached"; oid: Oid }
  | { kind: "unborn"; name: string };

interface RepoSnapshot {
  repoId: RepoId;
  workspaceKey: string;           // Stable exact worktree identity, chỉ dùng persistence UI
  version: Version;
  displayName: string;
  displayPath: string;            // Chỉ display, không round-trip để mutation
  head: HeadState;
  trust: "readOnly" | "trusted";
  state: "normal" | "merging" | "stashConflict" | "externalOperation";
  mergeOrigin: "app" | "external" | null;
  upstream: null | { refId: RefId; ahead: number; behind: number };
  lastFetchAt: string | null;      // ISO UTC, null nếu không biết
  activeOperation: OperationId | null;
  stagedCount: number | null;     // null khi trust gate chưa cho đọc worktree
  unstagedCount: number | null;
  conflictCount: number | null;
}

interface ChangedFile {
  pathId: PathId;
  displayPath: string;
  originalDisplayPath?: string;    // Rename source; token giữ cả old/new raw paths
  indexStatus: string;             // Enum normalize A/M/D/R/C/T/U hoặc space
  worktreeStatus: string;          // Kể cả ?; không suy một field từ field còn lại
  kind: "text" | "binary" | "symlink" | "submodule" | "unknown";
  conflicted: boolean;
}

interface RefItem {
  refId: RefId;
  fullName: string;
  label: string;
  kind: "local" | "remote" | "tag";
  oid: Oid;
  current: boolean;
  checkedOutElsewhere: boolean;
}

interface ConflictFile {
  pathId: PathId;
  displayPath: string;
  kind: "content" | "addAdd" | "modifyDelete" | "rename" | "binary"
    | "symlink" | "submodule" | "other";
  stages: { stage: 1 | 2 | 3; oid: Oid; mode: string }[];
  supportedActions: ("acceptCurrent" | "acceptIncoming" | "markResolved"
    | "resolveDeletion" | "externalOnly")[];
}

interface CommitRow {
  oid: Oid; parents: Oid[]; subject: string;
  authorName: string; authoredAt: string; committedAt: string;
  refs: RefId[]; boundary: boolean;
}
interface HistoryPage {
  historySessionId: string;
  rows: CommitRow[];
  nextCursor: string | null;
  truncated: boolean;
}
```

`PathId` gắn listing generation; token cũ trả `STALE_STATE`. Backend giữ bytes trên Unix, không dùng lossy UTF-8 để gọi Git. UI có thể render escaped display path, copy display path là tiện ích không phải mutation input. Repo có file non-UTF-8 phải vẫn stage đúng file.

Read-only trust chỉ cho object/ref/history reads đã được harden. `repo_status`, worktree diff và conflict working preview trả `TRUST_REQUIRED` trước khi đọc nếu repo chưa trusted; UI dùng null counts như “Unavailable until trusted”, không hiển thị zero giả.

## 3. Read commands

Mọi request có `requestId`; request liên quan repo có `repoId` trừ `repo_open`. Mọi read result repo-specific được bọc `Versioned<T>` trừ snapshot tự có version. Bảng bỏ các fields chung cho gọn.

| Command | Input thêm | Data |
|---|---|---|
| `app_preflight` | Không | Git path/version, platform, feature support |
| `repo_recent_list` | Không | Recent entries với display path và native folder handle nếu có |
| `repo_open` | `selectedPath: string` từ dialog | `RepoSnapshot`; open read-only trước trust |
| `repo_close` | `repoId` | `{closed: true}`; reject khi mutation chưa settle |
| `repo_snapshot` | `refresh: boolean` | `RepoSnapshot` |
| `repo_refs` | Không | `RefItem[]` |
| `repo_status` | Không | `{files: ChangedFile[]}` |
| `history_page` | `scope: allRefs \| head \| ref`, `refId?`, `cursor?`, `limit: 1..200` | `HistoryPage` |
| `history_search` | `scope`, `refId?`, `query: string`, `cursor?`, `limit` | `{rows, nextCursor, incomplete}`; literal search, không regex shell |
| `commit_details` | `oid: Oid`, `parentIndex?: number` | Commit metadata, parents, message, normalized file changes |
| `diff_read` | `target: DiffTarget` | `DiffDocument` |
| `stash_list` | Không | `{stashId, oid, label, createdAt}[]`; stashId map OID, không tin index ổn định |
| `conflict_list` | Không | `{files: ConflictFile[], canComplete, canAbort}` |
| `conflict_preview` | `pathId` | Base/current/incoming previews + working fingerprint + supported actions |
| `identity_read` | Không | Effective name/email, source scope, signing configured; không secrets |
| `settings_get` | Không | Settings v1 |
| `operation_get` | `operationId`, `requestId`; `repoId?` | Current operation record (clone/init có thể chưa có repoId) |

```ts
type DiffTarget =
  | { kind: "worktree" | "index"; pathId: PathId }
  | { kind: "commit"; oid: Oid; parentIndex: number | null; pathId: PathId };
interface DiffDocument {
  kind: "text" | "binary" | "symlink" | "submodule" | "tooLarge";
  displayPath: string;
  additions: number | null;
  deletions: number | null;
  hunks: { hunkId: string; header: string; oldStart: number; newStart: number;
    lines: { kind: "context" | "add" | "delete" | "noNewline";
      oldLine: number | null; newLine: number | null; text: string }[] }[];
  truncated: boolean;
  reason?: string;
}
```

`commit_details` file list cung cấp path tokens theo commit + selected parent; khi đổi parent gọi lại details với optional `parentIndex` và invalid old tokens. Parent index default 0 cho non-root; null chỉ hợp lệ cho root. Không resolve OID lấy từ UI thành revision expression tùy ý.

## 4. Mutation commands

Repo mutation dùng `WriteContext`; trả nhanh `{operationId}` sau validation/queue registration. Acceptance này **không phải** thành công Git. Init/clone có `requestId`, destination validation và destination mutex thay repo context. Settings/close/trust là lifecycle commands trả trực tiếp, không Git jobs.

| Command | Input thêm / constraints | Terminal result |
|---|---|---|
| `repo_init` | `selectedPath`, `initialBranch`; explicit confirmation UI | Snapshot + repoId |
| `repo_clone` | `urlOrPath`, `destination`; protocol allowlist, destination mới/rỗng | Snapshot + repoId hoặc partial destination status |
| `repo_trust_set` | `repoId`, `trusted: boolean`, user decision | Trust state; không tự trust từ open |
| `index_stage` | `pathIds: PathId[]` không rỗng | Fresh snapshot |
| `index_unstage` | `pathIds: PathId[]` | Fresh snapshot, kể cả unborn HEAD |
| `worktree_discard_file` | `pathId`, `confirmationToken`; chỉ unstaged side | Fresh snapshot; tracked restore từ index, untracked xóa exact file |
| `diff_hunk_stage` | `pathId`, backend-issued `hunkId`; worktree text diff | Fresh snapshot; apply exact hunk vào index |
| `diff_hunk_discard` | `pathId`, backend-issued `hunkId`, `confirmationToken` | Fresh snapshot; reverse exact hunk trong worktree |
| `commit_create` | `subject`, `body`; không shell/editor | New commit OID + snapshot |
| `branch_create` | `name`, `startOid`, `switchAfterCreate: boolean` | Ref + snapshot; partial outcome nếu create thành công nhưng switch lỗi |
| `branch_switch` | `refId`; local ref hoặc remote ref + explicit new local name | Fresh snapshot |
| `branch_delete` | `refId`, `confirmationToken` | Fresh snapshot; chỉ safe delete |
| `history_checkout` | `oid` | Detached HEAD tại exact commit; clean worktree |
| `tag_create_at` | `oid`, `name` | Lightweight tag; reject trùng tên |
| `history_push_to` | `oid`, explicit `remote` + `destBranch` | Normal push `oid:refs/heads/dest`; không force |
| `cherry_pick_onto` | `oid` | `--no-commit`; review staged result trước khi commit |
| `revert_commit` | `oid` | `--no-commit`; HEAD không revert chính nó |
| `merge_commit` | `oid` | `--no-ff --no-commit`; dùng chung flow complete/abort |
| `rebase_onto` | `oid`, `confirmationToken` (`history_rebase`) | Rebase branch hiện tại; conflict kết thúc ở terminal |
| `reword_message` | `oid` (= HEAD), `subject`, `body` | `commit --amend -F -` qua stdin |
| `modify_commit` | `oid` (= HEAD) | Fold staged changes vào HEAD; index rỗng bị từ chối |
| `edit_author` | `oid` (= HEAD), `name`, `email` | `commit --amend --author` |
| `split_commit` | `oid` (= HEAD), `confirmationToken` (`history_split`) | Mixed reset về parent; worktree giữ changes |
| `move_to_branch` | `oid`, `name`, `switchAfterCreate` | Branch tại OID + switch tùy chọn |
| `rebase_interactive_from` | `oid`, explicit `plan[]`, `confirmationToken` (`history_rebase_interactive`) | Scripted `GIT_SEQUENCE_EDITOR`; plan phải bao phủ đúng range |
| `reset_soft` / `reset_mixed` | `oid` | Dời branch pointer; giữ worktree (mixed unstages) |
| `reset_hard` | `oid`, `confirmationToken` (`history_reset_hard`) | Ghi đè index + tracked worktree; modal gõ lại short OID |
| `branch_move` | `refId`, `oid`, `confirmationToken` (`branch_move`) | `branch -f`; cấm current/checked-out-elsewhere |
| `branch_rename` | `refId`, `newName` | `branch -m`; validate tên, reject trùng |
| `branch_set_upstream` | `refId`, `upstreamRefId` (remote) | `--set-upstream-to`; upstream phải là remote ref |
| `branch_push` | `refId` | Push tới upstream đã cấu hình, normal only; chưa có upstream thì báo dùng Set upstream |
| `remote_fetch` | `remoteId` | Refreshed snapshot |
| `remote_pull` | `remoteId`, `upstreamRefId`; ff-only | Fresh snapshot hoặc divergence |
| `remote_push` | `remoteId`, `targetBranch`, `setUpstream: boolean`, `confirmedDestination: true` | Remote outcome + snapshot |
| `bitbucket_connect` | `repoId`, `expectedVersion`, optional `remote`, `apiToken`; trusted repo + Bitbucket Cloud HTTPS + configured credential helper | `{remoteName, username, credentialSaved}`; token không có trong response/log |
| `stash_save` | `message`, `includeUntracked: boolean` | Stash OID + snapshot hoặc no-change |
| `stash_apply` | `stashId`, `mode: apply \| pop` | Snapshot + retained stash flag |
| `merge_start` | `sourceRefId`, `confirmedTargetOid` | Merging snapshot, conflicted hoặc ready-to-complete |
| `conflict_accept` | `pathId`, `side: current \| incoming`, `workingFingerprint`, `confirmationToken` | File updated, **chưa stage** |
| `conflict_mark_resolved` | `pathId`, `workingFingerprint`, `resolution: workingFile \| deletion` | Stage exact file hoặc confirmed deletion + snapshot |
| `merge_complete` | `subject`, `body`; no unmerged entries | Merge commit OID + snapshot |
| `merge_abort` | `confirmationToken` | Snapshot hoặc failed with preserved state |
| `operation_cancel` | `operationId` và requestId; repoId optional | `{cancelRequested: boolean}`; terminal outcome truy vấn riêng |

`remoteId` từ command `remote_list` (input ReadContext; output `{remoteId, name, fetchUrlDisplay, pushUrlDisplay}[]`), URL hiển thị đã redact. `settings_update` dùng `{requestId, settingsVersion, patch}` → settings mới; whitelist fields. `repo_recent_remove` dùng saved entry ID → `{removed: true}`; chỉ xóa recent entry, không xóa folder. `workspaces_save` nhận `{entries: [{key, displayPath}], activeKey}` (gửi tối đa 100, lưu 20) → `{saved: true}`; key lạ/trùng bị loại, không chạm Git. `workspaces_restore` không input thêm → `{opened: RepoSnapshot[], skipped: [{key, displayPath, code, message}], activeKey}`; mục mở lỗi bị prune.

`confirmation_prepare` input `{...WriteContext, action, targets}` → `{confirmationToken, summary, expiresAt}`. Dùng cho branch delete, conflict overwrite, merge abort, discard file và discard hunk; token bind repo/version/action/target, một lần, TTL 60s. Whole-file discard còn bind fingerprint của worktree bytes; hunk discard bind `hunkId` sinh từ raw patch. Summary phải đủ tên/path/hậu quả để UI xin xác nhận cụ thể; backend reject nếu repo state/content thay đổi. Token không thay thế trust.

## 5. Operation lifecycle và events

```ts
type OperationState = "queued" | "running" | "cancelling"
  | "succeeded" | "failed" | "cancelled" | "unknown";
interface OperationRecord {
  operationId: OperationId; requestId: RequestId; repoId: RepoId | null;
  kind: string; state: OperationState;
  stage: string; progress: number | null;
  result: null | { snapshot?: RepoSnapshot; oid?: Oid; partial?: boolean };
  error: AppError | null;
  errorCode?: string | null; // compatibility field for old logs/frontends
}
```

Event `gitdock://operation` mang OperationRecord; `gitdock://repo-invalidated` mang `{repoId, version, reason}`. Terminal failure phải giữ structured `AppError` để UI render recovery; không thay bằng code-only string. Message là text an toàn đã redact, không chứa raw stderr, URL credential hoặc secret. Progress null khi Git không cung cấp tổng; UI indeterminate. Giới hạn event update 10 lần/giây/job.

Listener đăng ký trước submit; vẫn phải query operation sau submit/reconnect để tránh mất event terminal. Job registry giữ terminal record tối thiểu đến repo close hoặc session end; requestId đã nhận phải dedup cùng payload, khác payload trả `INVALID_ARGUMENT`. Không lưu requestId qua app restart để replay.

Cancel chỉ enabled theo phase: read/network có thể terminate process tree; trong final write/commit không hứa cancel ngay. Nếu process đã commit thành công khi cancel đến, terminal là `succeeded`. Sau interrupt rescan bắt buộc; không xác định được remote outcome → `unknown` + inspect, không auto retry push.

`bitbucket_connect` là secret-bearing command: request không implement debug logging; token chỉ được chuyển tới `git credential approve` bằng stdin, tối đa 4096 bytes và reject control characters. Command không đưa token vào argv/env/remote URL/app persistence. UI chỉ submit sau user click **Save & retry push**; retry push là một operation mới.

## 6. Contract tests bắt buộc

Round-trip tất cả union variants; camelCase; null vs optional; error transport; stale path/ref token; unsafe integer; invalid enum; OID SHA-1/SHA-256 shape; repo mismatch; missed terminal event; double submit; branch create partial success. Mock adapter dùng cùng DTO/schema; không trả random fake success để vượt test.
