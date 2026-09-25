# Development

## Kiến trúc

App dùng Vite SPA, Svelte 5 runes và TypeScript strict. `src/app/` ghép workspace; `src/lib/components/` chứa UI; `src/lib/styles/tokens.css` là nguồn design tokens duy nhất.

Frontend gọi typed IPC qua [client.ts](../src/lib/ipc/client.ts), chọn [real adapter](../src/lib/ipc/real.ts) trong Tauri hoặc [mock adapter](../src/lib/ipc/mock.ts) cho browser demo. Hợp đồng dữ liệu nằm trong [types.ts](../src/lib/ipc/types.ts) và Rust DTOs ở [domain](../src-tauri/src/domain/) / [commands](../src-tauri/src/commands/). Khi sửa payload, cập nhật hai phía và kiểm tra JSON serialization.

Backend chia thành `commands/` (IPC handlers), `git/` (system Git), `services/` (session/job registry), `domain/` (DTO/error) và `persistence/` (settings). Entry point [lib.rs](../src-tauri/src/lib.rs) đăng ký commands. Không đưa shell hoặc API nhận argv tùy ý ra frontend.

## Git và dữ liệu

- Repository, filenames, config, commit messages và remote output là dữ liệu không tin cậy. Rust xác thực payload và path tokens; UI render plain text. Không coi cache frontend là Git truth.
- Dùng system Git qua [runner.rs](../src-tauri/src/git/runner.rs), fixed executable/argv, không shell. Giữ trust gate, sanitization, timeout/output bounds và redaction. Không tự sửa global `safe.directory`, xóa lock hoặc replay mutation khi kết quả chưa rõ.
- Mutation phải dùng repo/version/action/path identity đã được backend kiểm tra, queue theo common directory, kiểm tra lại trước khi ghi và refresh sau khi hoàn tất. Confirmation phải gắn target cụ thể; discard/hunk phải kiểm tra fingerprint/token.
- Settings và workspaces lưu local qua [store.rs](../src-tauri/src/persistence/store.rs), giữ schema migration và atomic writes. Không đổi identifier/storage namespace chỉ để đổi branding; linked worktrees phải giữ workspace identity riêng.
- Không log hoặc lưu credential/token vào app settings. Git helper/SSH agent quản lý credential; token nhập qua UI chỉ được chuyển qua luồng typed command có kiểm soát. Test Git mutations bằng repositories tạm, không dùng repository của người dùng.

## Kiểm tra

Các lệnh check/build nằm trong [README](../README.md#kiểm-tra-và-build). Vitest dùng `tests/unit/`; Rust có tests cạnh implementation; `tests/ui/` giữ các fixtures để kiểm tra browser. [Release tests](../tests/release/check-release.py) dùng filesystem tạm và `gh` giả, không publish thật.

Với UI, kiểm tra màn hình đang chạy, empty/loading/error states và keyboard; không chỉ dựa vào demo data hoặc ảnh. Với Git/IPC, kiểm tra behavior trên repo tạm và trường hợp stale state/error. Báo chính xác check đã chạy và blocker; browser demo không chứng minh native Git workflow.

## Trạng thái kiểm chứng

Cập nhật 2026-09-25. Các kết quả dưới đây là những checks đã chạy trong phiên phát triển, không thay thế một lần CI trên GitHub:

- Frontend: 128 unit tests pass; ESLint pass; Svelte/TypeScript không có errors, còn 19 warnings CSS/a11y. Production frontend build đã pass.
- Rust: 145 tests đã pass (144 trong sandbox và một test HTTP stub rerun với quyền loopback); fmt/clippy pass.
- Branding: browser Welcome/workspace và native launch từ `.deb` đã kiểm tra tên Octopus/icon. `.deb` và `.rpm` build pass; AppImage đã tạo và kiểm tra nội dung, nhưng lần bundle cuối exit 1 ở post-processing vì executable đang chạy (`Text file busy`).
- Release tooling: actionlint, Bash/YAML syntax và 15 offline release tests pass. GitHub-hosted build/publish end-to-end **chưa chạy**, cần lần trigger trên GitHub.
- Native acceptance còn thiếu: đọc repo, stage/commit, sync/merge/conflict, nhiều repo/restart trên native UI; Bitbucket/remote thật; frame/memory profiling và kiểm chứng Windows/macOS. Native milestone M1–M4 vẫn chưa được nghiệm thu. Release tự động không đổi trạng thái này.

Lượt dọn project ngày 2026-09-25: đã thay 112 file tài liệu cũ bằng hai tài liệu hiện tại, bỏ script test chưa triển khai và sửa references/config liên quan. Đã chạy lại: 128 unit tests, 15 release tests, Svelte/TypeScript (0 errors, 19 warnings), lint, production frontend build, Rust fmt, Bash syntax và actionlint — tất cả pass. Đã kiểm tra 30 local Markdown links/anchors và các native icon paths; không còn references đến tài liệu đã xóa. Rust chỉ đổi comments nên không chạy lại Rust tests/native bundle trong lượt dọn này. Nội dung trước dọn được backup ngoài repository; Git index và các thay đổi khác của người dùng được giữ nguyên.

Khi bàn giao, cập nhật mục này với thay đổi, checks và blocker hiện tại; không tạo thêm thư mục evidence/handoff theo từng task. Chỉ thêm tài liệu mới khi cần hướng dẫn bảo trì lâu dài.

## Icon

Master: [public/brand/octopus.png](../public/brand/octopus.png). Hình bạch tuộc mint trên nền charcoal, xúc tu gợi các node/nhánh Git, không chữ. Icon được tạo bằng image generation và đã áp dụng cho launcher, Welcome, repository tabs và favicon.

Tạo lại desktop icons vào thư mục tạm để review trước khi copy:

```sh
pnpm tauri icon public/brand/octopus.png --output /tmp/octopus-generated-icons
```

Copy bộ desktop PNG/ICO/ICNS ở root output vào `src-tauri/icons/`, `128x128.png` sang `public/brand/octopus-128.png`, `32x32.png` sang `public/favicon.png`. Các paths bundle được khai báo trong [tauri.conf.json](../src-tauri/tauri.conf.json).
