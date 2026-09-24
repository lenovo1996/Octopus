# Handoff — graph UI và main-panel diff

- Ngày: 2026-09-23.
- Trạng thái: implementation + unit/integration + browser QA + native build hoàn tất; native visual/workflow cho fix mới chưa chạy.
- Phạm vi: follow-up T05/T08 theo yêu cầu user; không đổi backend/IPC/dependencies.

## Thay đổi

1. `src/lib/graph/layout.ts`: lane carry khởi tạo đầy đủ từ page trước; rail đi xuyên hàng tách khỏi incoming half-rail/cạnh parent. Mỗi parent có edge, root không nối xuống, tip không nối lên. Unit tests kiểm tra topology và mọi điểm chia page.
2. `HistoryPane.svelte`: vẽ cạnh cong, marker merge theo số parent, gutter tối thiểu, node/row alignment; scroll ngang đồng bộ header, metadata theo container width, keyboard reveal row. Sửa demo parent OIDs để fixture thực sự nối graph.
3. `DiffPane.svelte` + `diff/controller.ts`: main-panel viewer có ×/Escape, old/new line numbers, loading/error/retry/empty/fallback/truncation/no-final-newline. Request generation bỏ response cũ sau đổi file/đóng/đổi repo.
4. `App.svelte` + `Inspector.svelte`: file list giữ ở inspector; click truyền exact pathId và `index`/`worktree`/`commit` đúng nguồn. Main panel giữ HistoryPane mounted khi diff mở để khôi phục scroll/selection. Đổi commit/parent/inspector/scope đóng diff; details requests dùng latest-request guard. Close trả keyboard focus về file opener còn tồn tại.
5. README/progress/UI spec được đồng bộ với trạng thái implementation thật và yêu cầu diff mới. Wireframe gốc được giữ nguyên.

## Kiểm chứng

| Check | Kết quả |
|---|---|
| `pnpm check` | 0 errors, 0 warnings |
| `pnpm lint` | pass |
| `pnpm test:unit` | 57 tests / 13 files pass |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 113 Rust tests pass |
| `pnpm build` | pass |
| `pnpm tauri build --no-bundle` | pass; binary `src-tauri/target/release/gitdock` |
| Browser demo 1280×800, 1440×900 | graph/diff, ×/Escape, staged source, parent switch pass |
| Browser graph fixture 10k commits/24 lanes | 26–27 rendered rows; alignment, horizontal header, keyboard, hide/show + resize pass |
| Native visual + real-repo IPC workflow | **blocked/not run**: chỉ có browser automation trong phiên; cần native manual acceptance |

Bằng chứng chi tiết: [QA](../evidence/UI-graph-diff-2026-09-23/qa.md), [Rust output](../evidence/UI-graph-diff-2026-09-23/rust-tests.txt), [native build output](../evidence/UI-graph-diff-2026-09-23/native-build.txt), [checksums](../evidence/UI-graph-diff-2026-09-23/sha256.txt).

## Trạng thái project

- T01–T16 đã có code; T15/T16 chưa đủ tất cả acceptance gates. T16 handoff đã ghi nhận MIT và clean-install trong container; các thông tin cũ về chưa scaffold/chưa license không còn là trạng thái hiện tại.
- Git worktree hợp lệ nhưng branch `main` chưa có commit; files đang untracked. Không commit/push trong phiên này.
- Native binary mới đã build. Các installer `.deb/.rpm/.AppImage` cũ **chưa được rebuild** trong fix này; chạy trực tiếp binary mới hoặc `pnpm tauri dev`.
- `test:ui`, `test:visual`, `test:native` trong package scripts vẫn là placeholder; không báo các lệnh này pass. Fixture browser là trang QA riêng, không được import vào production entry.

## Kiểm tra native tiếp theo (ước lượng 2 phút)

Mở `src-tauri/target/release/gitdock` → chọn repo có merge → click commit/file → xác nhận diff ở main panel và × quay lại graph. Trong Working changes, click cùng file ở Staged/Unstaged để xác nhận đúng source. Chỉ đọc; không cần stage/commit để kiểm tra fix.
