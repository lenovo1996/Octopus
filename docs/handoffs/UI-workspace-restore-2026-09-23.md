# Handoff — Workspace restore (T19)

- Ngày: 2026-09-23. Yêu cầu trực tiếp của user: tắt app rồi bật lại mất hết workspace đã mở; muốn lưu trạng thái này.
- App nay tự lưu tab order + tab active (debounce 400ms) và tự mở lại khi launch.

## Thay đổi

1. Backend: `StoreData` thêm `openWorkspaces` (tối đa 20, giữ thứ tự) và `activeWorkspace` trong cùng file settings atomic; migration cũ nhờ `#[serde(default)]`. `workspaces_save` validate key `worktree-v1:<hex>`, loại trùng/lạ, cap 20. `workspaces_restore` decode key thành path rồi mở qua `core_open` (discovery + trust cache + recents đầy đủ), trả `{opened, skipped: [{key, displayPath, code, message}], activeKey}`, prune mục mở lỗi. Đăng ký trong `lib.rs`.
2. Frontend: `App.svelte` restore một lần khi mount (native only, có trạng thái "Restoring workspaces…"), chọn tab theo saved active; `$effect` lưu sau restore; repo mất hiện notice dismissible một lần. Helpers thuần trong `tabs.ts` (`openWorkspaceEntries`, `activeWorkspaceKey`, `resolveRestoredActive`); `real.ts` + `mock.ts` đủ 2 methods.
3. Draft theo worktree không đổi vì workspaceKey ổn định; đóng hết tabs lưu rỗng nên launch sau về Welcome đúng.
4. Docs: backlog T19, progress T19, IPC contracts, security/data (T19).

## Kiểm chứng và giới hạn

- `cargo test`: **135/135 pass** (3 tests mới: save/restore round-trip, skip missing + prune, forged keys). `cargo clippy -D warnings`, `cargo fmt --check` sạch.
- `pnpm check` 0 errors, `pnpm lint` sạch, `pnpm test:unit` **84/84 pass** (21 files; `workspace-restore.test.ts` mới), `pnpm build` pass, `pnpm tauri build --no-bundle` ra binary release.
- Native restart workflow **blocked/not run**: chưa tắt/bật app desktop thật để kiểm chứng tabs mở lại trong phiên này; M1–M4 không đổi.
- Không commit/push/install/publish. Source vẫn untracked trên `main`.

Thao tác tiếp theo (ước lượng 2 phút): mở binary mới, mở 2 repo, active tab thứ hai, tắt app hẳn rồi bật lại; kiểm tra cả 2 tabs mở lại đúng thứ tự và đúng tab active, rồi thử di chuyển một repo và restart để thấy notice + prune.
