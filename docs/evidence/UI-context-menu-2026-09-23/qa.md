# Context menu QA — 2026-09-23

Phạm vi: browser demo qua CUA tại 1280×800; không phải native WebKit/Git workflow. Mutation chỉ chạy trên mock adapter, không chạm repository thật.

## Hành vi đã kiểm tra

1. Right-click repository tab: menu có Current repository disabled, Copy repository path, Open another repository…, Close repository tab. End focus đúng item cuối; Escape đóng menu.
2. Right-click commit `c9f1a2b`: Open commit details, Copy commit ID, Copy subject. Open commit details chuyển inspector đúng commit.
3. Right-click changed file trong Commit details: Open diff và Copy relative path. Open diff hiển thị File diff trong main panel với Close diff.
4. Right-click `new notes.txt` trong Unstaged: Open diff, Stage file, Select file, Copy relative path. Stage file chạy mock mutation; counts đổi Unstaged 2/Staged 2 → Unstaged 1/Staged 3 và file chuyển sang Staged.
5. Right-click action tại x gần mép phải: menu rect left 1062/right 1272 trong viewport width 1280, giữ margin 8px.
6. Shift+F10 mở cùng menu trên repository tab và file. ArrowDown ×2 + Enter chọn Select file; checkbox staged `new notes.txt` chuyển checked.
7. Menu nguồn có highlight rõ; disabled item không nhận focus. Không có console warning/error sau chuỗi kiểm tra.
8. Right-click commit `e5f6071` @1280×720: đủ 17 Git actions theo đúng thứ tự và 4 separator; 16 mục chưa có typed engine hiện `Planned`, hard reset dùng danger color; menu nằm trọn viewport và có overflow dọc.
9. Regression pass lần cuối: TopBar branch button mở modal `Starting from HEAD c9f1a2b3c9f1`; commit action mở `Starting from commit e5f60718e5f6`. Bỏ Switch after create, tạo `qa/regression`; sidebar và graph badge cùng hiện ref mới tại `e5f6071`, HEAD vẫn `main`; console error/warning rỗng.

## Automated checks

- `pnpm check`: 0 errors / 0 warnings.
- `pnpm lint`: pass.
- `pnpm test:unit`: 72 tests / 17 files pass, gồm exact commit-action order/grouping, planned reasons, hard-reset danger, viewport clamp, wrap qua disabled items và Context Menu/Shift+F10 recognition.
- `pnpm build`: pass.
- Final `pnpm tauri build` ngoài sandbox: exit 0, tạo executable + `.deb`/`.rpm`/`.AppImage`. Lần sandbox trước đó lỗi tại linuxdeploy vì cần tải AppImage runtime. Xem `sha256.txt` và `source-sha256.txt`.
- Rust/IPC không đổi; không chạy lại Rust suite. Baseline gần nhất 118 pass.
- Native UI trên repo thật: **blocked/not run** vì phiên không có native window automation. Browser demo không nâng M1–M4 gates.
