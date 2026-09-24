# Handoff — native diff, history columns và inspector UX

- Ngày: 2026-09-23; follow-up T05/T08/T09 theo yêu cầu user.
- Implementation, frontend/backend checks, browser QA và native packaging hoàn tất; native visual/workflow **blocked/not run** trong phiên này.
- Giữ Rust/Tauri 2/Svelte 5/TypeScript; không thêm dependencies hay đổi Git engine mutation semantics.

## Lỗi native diff

`src-tauri/src/domain/repos.rs`: `DiffTarget` chỉ có field-level camelCase ở từng variant, thiếu enum-level rename. JSON từ frontend `{ "kind": "worktree", "pathId": "…" }` bị Serde từ chối trước khi `diff_read` chạy, vì Rust chờ `Worktree`, `Index`, `Commit`. Frontend nhận rejection string và trước đây chỉ báo chung `IO_ERROR: Native call failed`.

Đã tái hiện bằng regression test trước fix: `unknown variant 'worktree', expected one of 'Worktree', 'Index', 'Commit'`. Thêm `rename_all = "camelCase"` vào enum, giữ fields camelCase. Không thay frontend contract. `client.ts` xử lý native string rejection bằng thông báo an toàn và hướng dẫn restart; không đưa raw error/payload ra UI.

Tests deserialize đúng JSON frontend cho worktree/index/commit, merge parent và root. Integration test dùng repo tạm tạo base → staged → unstaged rồi đọc diff qua request đã deserialize; xác minh staged/unstaged có nội dung khác nhau. Commit integration test cũng dùng JSON request.

## Giao diện

1. History: Branch bên trái Graph; local/origin phân biệt tại ref tip. Branch/Graph/Subject/Author cùng grid, resize bằng chuột hoặc keyboard, double-click/Home reset, localStorage persistence. Graph minimum theo lane count; Subject mặc định responsive; Author không tự ẩn khi center hẹp.
2. Working changes: header branch/count/refresh, filename/path tách dòng, Unstaged/Staged có count/action nhóm và action từng file. Selection dùng source + exact pathId, không display path. Không còn fake fallback khi status chưa load. Footer commit có summary/description/identity, disable reason và Ctrl/Cmd+Enter giới hạn trong editor.
3. Commit details: subject/full OID/copy/ref labels, author/date, body, chọn parent và View parent; bộ lọc file; Back to working changes cố định. Click file mở main diff, đổi parent đóng diff cũ.

Files chính: `HistoryPane.svelte`, `ColumnResize.svelte`, `WorkingChangesPanel.svelte`, `CommitDetailsPanel.svelte`, `FileChangeRow.svelte`, `Inspector.svelte`, `App.svelte`, `history/columns.ts`, `history/refs.ts`, `status/selection.ts`, `ipc/client.ts`, backend `domain/repos.rs` và `commands/diff.rs`.

## Kiểm chứng

| Check | Kết quả |
|---|---|
| `pnpm check` | 0 errors, 0 warnings |
| `pnpm lint` | pass |
| `pnpm test:unit` | 63 tests / 14 files pass |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 115 tests pass |
| `cargo clippy --locked --manifest-path src-tauri/Cargo.toml -- -D warnings` | pass |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | pass |
| `pnpm build` | pass |
| `pnpm tauri build` | release executable + `.deb`/`.rpm` thành công; AppImage bị chặn tải runtime trong sandbox |
| `pnpm tauri bundle --bundles appimage --verbose` với quyền mạng | pass; tạo AppImage từ executable vừa build |
| Browser demo 1280×800/1440×900 | resize/persistence, source selection, stage/unstage, copy/filter/parent/diff pass |
| Native window + real repository UI flow | **blocked/not run**: chỉ có browser automation; Rust integration tests không thay thế native E2E |

Evidence: [QA](../evidence/UI-native-diff-inspector-2026-09-23/qa.md), [Rust output](../evidence/UI-native-diff-inspector-2026-09-23/rust-tests.txt), [native build](../evidence/UI-native-diff-inspector-2026-09-23/native-build.txt), [AppImage build thành công](../evidence/UI-native-diff-inspector-2026-09-23/appimage-build.txt), [artifact checksums](../evidence/UI-native-diff-inspector-2026-09-23/sha256.txt), [source checksums](../evidence/UI-native-diff-inspector-2026-09-23/source-sha256.txt). `test:ui/test:visual/test:native` vẫn là placeholder; không báo chúng pass. Source vẫn untracked trên `main` chưa có commit; không commit/push trong phiên.

## Build artifacts mới

- `src-tauri/target/release/gitdock`
- `src-tauri/target/release/bundle/deb/GitDock_0.1.0_amd64.deb`
- `src-tauri/target/release/bundle/rpm/GitDock-0.1.0-1.x86_64.rpm`
- `src-tauri/target/release/bundle/appimage/GitDock_0.1.0_amd64.AppImage`

`dpkg-deb --info` đọc gói Debian thành công. Các artifacts trên chứa fix mới, thay cho bản cũ của lần graph/diff trước. AppImage cần tải runtime từ GitHub; lần chạy sandbox thất bại tại bước đó, chạy lại với quyền mạng đã thành công. Chưa cài các gói này vào hệ thống hay nghiệm thu clean-install trong phiên fix này.

## Kiểm tra tiếp theo — ước lượng 2 phút

Thoát app cũ rồi mở `src-tauri/target/release/gitdock`. Chọn commit → click file → xác nhận diff → nhấn ×; kéo ranh giới Graph/Subject/Author. Native binary cũ không nhận được sửa Serde bằng frontend hot reload.
