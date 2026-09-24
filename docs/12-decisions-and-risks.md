# Decisions and risks

Ngày baseline: **2026-09-22**. “Accepted” dưới đây nghĩa là baseline thiết kế cho AI triển khai; chỉ Linux-first và Rust/Svelte/reference layout là yêu cầu người dùng xác nhận trực tiếp. Các lựa chọn còn lại là đề xuất đã ghi rõ, có thể đổi bằng ADR mới.

## ADR register

| ID | Status | Decision | Trade-off / khi xem lại |
|---|---|---|---|
| ADR-001 | Accepted baseline | Tauri 2 + Svelte 5/TS/Vite SPA + Rust | Native webview khác nhau theo OS; xem lại nếu có blocker chứng minh được, không đổi theo sở thích agent |
| ADR-002 | User-confirmed | Linux trước, Windows/macOS sau | Giảm matrix MVP; portability vẫn cần test riêng |
| ADR-003 | Accepted baseline | Git CLI adapter, chưa git2/libgit2/gix | Tương thích Git config, có system dependency và process parsing; profile trước khi đổi provider |
| ADR-004 | Accepted baseline | Three-pane dark graph-centered UI | Theo ảnh mẫu; colors/spacing là thiết kế mới, không pixel clone |
| ADR-005 | Accepted baseline | Whole-file stage/unstage, unified diff | Scope nhỏ và dễ xác thực; hunk staging P2 cần patch validation |
| ADR-006 | Accepted baseline | Pull ff-only; merge explicit --no-ff --no-commit | Không auto rewrite/stash; explicit merge tạo merge commit kể cả trường hợp có thể ff |
| ADR-007 | Accepted baseline | No universal undo/redo, PR integration/submodule mutation P2 | UI không khớp đủ mọi action trong ảnh ở MVP; có nhãn Planned/Read-only |
| ADR-008 | Accepted baseline | Local JSON settings + no cloud/no telemetry | Đủ state nhỏ; đổi sang DB khi có dữ liệu bền vững lớn và migration plan |
| ADR-009 | Accepted baseline | Read-only open + repo trust before mutation/network | Tránh tự chạy executable Git config; cần UX giải thích rõ, không chặn đọc history bình thường |
| ADR-010 | Superseded by user request 2026-09-23 | Nhiều repo trong một cửa sổ qua tab, một instance | Mỗi repo có workspace riêng; canonical worktree dedup, linked worktrees độc lập và chung write queue; xem T17 |

## Câu hỏi có default, chưa chặn scaffold

| Chủ đề | Default hiện tại | Khi cần quyết định |
|---|---|---|
| Tên/branding | GitDock | Trước public release; không mua domain hoặc thiết kế logo khi chưa yêu cầu |
| License sản phẩm | Chưa chọn, không tự gắn MIT/proprietary | Trước publish source/binary ra công chúng |
| Distro/architecture | Ubuntu 24.04 LTS x86_64 | T01 preflight/T16 packaging; thêm distro là scope mới |
| UI language | English, strings tách để localize | T02; tiếng Việt là P2 nếu chưa yêu cầu khác |
| Hosting/CI | Local scripts trước, GitHub Actions nếu dùng GitHub | Khi repository remote được biết; không tự tạo remote |

## Risk register

| ID | Mức | Risk | Mitigation / owner task |
|---|---|---|---|
| R01 | High | Stage/unstage nhầm filename có ký tự đặc biệt | Byte-safe parser + opaque paths + exact index tests; T07–T09 |
| R02 | High | Stale UI hoặc external Git dẫn tới write sai | Common-dir queue, recheck, no auto retry, refresh; T07/T15 |
| R03 | High | Merge abort/resolve làm mất local edits | Clean precondition, file fingerprint, confirmation, no reset fallback; T13 |
| R04 | High | Malicious config/hooks/remote output | Trust gate, safe reads, transport allowlist, plain text, redaction; T03/T11/T14 |
| R05 | High | Graph nối sai parent qua pagination | Fixed history session, lane carry, topology fixtures; T04/T05 |
| R06 | Medium | System Git/credential helper/version khác | Preflight, pinned test baseline, actionable auth failure; T01/T11/T16 |
| R07 | Medium | WebKit/driver environment làm native tests khó chạy | Early native smoke và separate test build; T01/T15 |
| R08 | Medium | Repo lớn hoặc diff khổng lồ treo UI | Streaming/limits/virtualization/cancel, benchmark thật; T04/T08/T15 |
| R09 | Medium | MVP phình ra để bắt chước mọi toolbar icon | PRD P0/P1/P2, disabled reasons, task scope; mọi task |
| R10 | Medium | Generated code và docs lệch contract | Rust canonical DTO + generation/parity tests + handoff review; T01/T15 |

## ADR template ngắn

Thêm một mục có ID mới: bối cảnh → quyết định → alternatives đã xem → trade-offs → affected FR/tasks/contracts → validation → ngày/người quyết định. Đánh dấu ADR cũ superseded nếu cần; không sửa lịch sử để giả lựa chọn mới đã luôn tồn tại.
