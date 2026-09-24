# Security and local data

## Trust boundary

Repository, filenames, commit messages, Git config và remote output đều là dữ liệu không tin cậy. UI chỉ render plain text; không HTML markdown tự do cho message/diff, không execute links hoặc code từ repo. Rust xác thực lại mọi payload IPC, kể cả payload do UI đã validate.

Open repo mặc định **read-only trust**. User chọn “Trust this repository” để cho phép mutation và network, với giải thích ngắn rằng Git hooks, filters, credential helpers và signing tools cấu hình trên máy có thể chạy. Đây là hành vi sản phẩm cần xây; không phải yêu cầu phê duyệt thêm để soạn tài liệu này. Trust không cho phép destructive actions ngoài PRD.

Read-only mode chỉ đọc refs/objects/history bằng allowlist, tắt fsmonitor/external diff/textconv; status/worktree diff chờ trust để tránh content filters ngoài ý muốn. Partial/promisor clone từ chối ở MVP trước object access; không tự lazy-fetch trong một thao tác đọc. Chỉ hỗ trợ protocol HTTPS/SSH/local có validate. Chặn unknown remote helpers dù repo đã trusted trong MVP. Trust cache gắn canonical worktree/common-dir identity; nếu repo bị thay thế hoặc identity thay đổi, hỏi lại; không chỉ nhớ display path.

## Tauri permissions

- Chỉ main local window có app commands cần thiết. Không remote web content hoặc iframe có IPC.
- Không expose generic shell/filesystem API cho frontend; dialog plugin chỉ chọn file/folder, opener chỉ explicit URL schemes cho Help.
- Cấu hình custom-command permissions qua app manifest/permissions và capability scope; không giả “thêm capabilities file” sẽ tự chặn mọi handler custom.
- CSP release chặn arbitrary script/network; assets local. Dev-server allowances chỉ trong dev config.
- Test-only automation plugins/commands/server không compile vào release. CI kiểm release features/permissions và absence của automation listener.

Tauri mặc định cho các app-registered commands dùng từ các windows/webviews; hạn chế chúng cần cấu hình app manifest phù hợp. Capability và backend validation bổ sung cho nhau. [Tauri capabilities](https://v2.tauri.app/security/capabilities/)

## Git/path protections

| Risk | Control |
|---|---|
| Shell/option/pathspec injection | Fixed subcommands, argv list, validated refs, literal path handling, no shell |
| Ghi nhầm file ngoài root | Backend path tokens + traversal reject; no symlink follow khi conflict writes; revalidate fingerprint |
| Repo đổi khi user confirm | Confirmation token bind repo/version/action/target, TTL; discard bind content fingerprint/hunkId và recheck trước execution |
| External Git race | Queue common-dir + recheck + native locks + postcondition scan; không xóa locks |
| Untrusted config/process | Trust gate, safe read overrides, allowed transport, không chạy external diff |
| Không biết operation đã thành công | Request dedup + operation lookup + refresh; không tự replay mutations |

Không tự thay đổi safe.directory toàn cục để vượt ownership error. Báo lỗi và để người dùng xử lý đúng repo. Đối với symlink read, preview link target text/metadata, không follow sang arbitrary filesystem.

## Persistence schema v1

Lưu trong app-specific config/data directory do Tauri/path API cung cấp; không hardcode `~/.config` cho mọi OS. Atomic temp-write + rename, permissions chỉ user nơi OS hỗ trợ. Settings schema version và migration bắt buộc.

```json
{
  "schemaVersion": 1,
  "settingsVersion": 1,
  "theme": "dark",
  "fontSizePx": 13,
  "panelWidths": { "sidebar": 220, "inspector": 380 },
  "recentRepositories": [],
  "trustedRepositories": [],
  "commitDrafts": {},
  "lastSelectedRepository": null
}
```

Repo IDs IPC sống trong session. Persistence dùng stable local repository key, không lưu repoId tạm. Recent list tối đa 20; draft tối đa 64 KiB/repo, giữ đến commit success hoặc user discard; draft có thể chứa thông tin riêng nên không vào diagnostics. Corrupted settings backup tại chỗ rồi default, thông báo recovery; migration không xóa draft âm thầm.

T17: frontend draft store `gitdock.drafts.v2` dùng `RepoSnapshot.workspaceKey` do Rust tạo từ canonical worktree identity (exact path bytes). Key chỉ dùng local UI persistence, không được nhận thay repoId/pathId trong Git commands. Giữ nguyên store v1; đọc fallback chỉ khi session id cũ còn khớp, không đoán draft của repo khác. Recent entries mới dùng worktree key để linked worktrees không đè nhau; legacy recent cùng display path được thay khi mở lại. Trust và write queue vẫn theo common-dir key. Tab close không xóa draft hoặc dữ liệu Git.

T19: store v1 thêm `openWorkspaces` (tối đa 20, thứ tự tabs) và `activeWorkspace` trong cùng file settings, ghi atomic như recents. Key là worktree key backend đã phát hành; frontend gửi key về, backend decode hex thành path rồi mở lại qua discovery + validation đầy đủ — key giả hoặc path không còn là repo bị bỏ qua, prune khỏi danh sách đã lưu, recents giữ lại làm fallback mở tay. Reopen giữ trust từ trust cache; không tự restore tabs khi repo đang busy vì session mới không có operation nào.

| Dữ liệu | Lưu? | Retention |
|---|---|---|
| Preferences/recents/trust/draft | Có, local | User xóa/reset; không sync cloud |
| Git objects/index/config | Git sở hữu | App không lưu bản thứ hai làm truth |
| Diff/history cache | Memory | Repo close/session invalidation |
| Job registry | Memory | Session; không replay qua restart |
| Logs | Local redacted | Rotate 5 files × 2 MiB, user clear |
| Credentials/access tokens/SSH key | Không trong app | Chỉ chuyển token do user nhập tới configured Git helper qua stdin; helper/agent quản lý retention |

## Diagnostics và recovery

Diagnostics opt-in export gồm app/OS/Git versions, error codes, timings, anonymized repo identifiers. Không đưa absolute repo path, remote credentials, commit message, author email hoặc file contents vào mặc định. User preview trước export; app không gửi network feedback tự động.

Trước branch delete, conflict overwrite, merge abort, discard file hoặc discard hunk: confirmation cụ thể với target/hậu quả; default button Cancel. Discard file tracked chỉ restore worktree từ index; untracked chỉ clean một exact literal file, không directory/ignored/all. Không có universal undo. Failure/cancel không xóa partial clone, không reset repository, không drop stash. Nếu operation result unknown, UI hướng dẫn Refresh/inspect và giữ draft. App restart detect Git operation state, không replay job cũ.
