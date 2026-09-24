# GitDock — Git GUI desktop

**Chạy desktop từ source:** `pnpm tauri dev`. **Browser demo:** `pnpm dev` (mock data, không thao tác Git thật).

GitDock là tên làm việc cho ứng dụng Git GUI desktop dùng **Rust + Tauri 2 + Svelte 5 + TypeScript**. Ưu tiên **Linux**, sau đó Windows/macOS. Thiết kế theo [example.webp](example.webp): sidebar trái, commit graph ở giữa, inspector phải và toolbar trên cùng.

**Trạng thái ngày 23/09/2026:** đã triển khai ứng dụng qua T16 và mở rộng nhiều repo theo yêu cầu user. Dùng **+ Open repo** để mở thêm tab, chuyển giữa các repo mà vẫn giữ draft, search và commit diff. Graph resize/scroll, context menu, create-branch-at-commit và Bitbucket Cloud API token flow đã hoàn tất. Working changes dùng direct actions không checkbox; có confirmed Discard file, Stage hunk và Discard hunk an toàn bằng backend-issued tokens. `77` frontend tests và `124` Rust tests pass. Native workflow/visual gates chưa được nghiệm thu đầy đủ; xem [progress](docs/progress.md) và [handoff mới nhất](docs/handoffs/UI-working-changes-hunks-2026-09-23.md).

## 1. Đọc theo mục đích

| Bạn cần | Tài liệu |
|---|---|
| Hiểu sản phẩm và phạm vi | [Overview](docs/00-project-overview.md), [PRD](docs/01-product-requirements.md) |
| Xây đúng giao diện ảnh mẫu | [UI/UX spec](docs/02-ux-ui-spec.md), [wireframe](docs/design/README.md), [design tokens](docs/design/tokens.css) |
| Thiết kế và nối Rust–Svelte | [Architecture](docs/03-architecture.md), [IPC contracts](docs/04-ipc-contracts.md), [Git engine](docs/05-git-engine.md) |
| Giao việc và theo dõi AI | [Plan](docs/06-implementation-plan.md), [backlog](docs/07-task-backlog.md), [execution guide](docs/11-ai-execution-guide.md), [progress](docs/progress.md) |
| Nghiệm thu và phát hành | [QA](docs/08-testing-and-acceptance.md), [security/data](docs/09-security-and-data.md), [build/release](docs/10-build-release.md), [decisions/risks](docs/12-decisions-and-risks.md), [sources](docs/13-reference-sources.md) |

## 2. Giao AI làm việc

1. Mở project trong coding agent và gửi toàn bộ [00-start.md](docs/prompts/00-start.md).
2. Sau mỗi task, dùng [01-execute-task.md](docs/prompts/01-execute-task.md) để giao task kế tiếp đã đủ dependency.
3. Dùng [02-review.md](docs/prompts/02-review.md) để kiểm tra chức năng; dùng [03-ui-check.md](docs/prompts/03-ui-check.md) tại các mốc UI.

Mỗi task phải cập nhật [progress.md](docs/progress.md), nêu rõ kiểm tra đã chạy, kết quả thực tế và blocker. Không đánh dấu `done` chỉ vì có code hoặc ảnh mockup.

## 3. Thứ tự quyết định khi tài liệu khác nhau

Yêu cầu mới nhất của chủ project → [AGENTS.md](AGENTS.md) → PRD về phạm vi → UI spec về giao diện → IPC/Git engine về hành vi → backlog về thứ tự. AI sửa tài liệu liên quan trong cùng task khi thay đổi quyết định; không âm thầm chọn một bản khác.

**Kiểm tra nhanh, dưới 2 phút:** chạy `pnpm tauri dev`, sửa hai vùng xa nhau trong một tracked text file, click file ở **Unstaged**, rồi chọn **Stage hunk** cho một vùng. Diff phải refresh và vùng còn lại vẫn ở Unstaged.
