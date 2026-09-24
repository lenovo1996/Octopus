# QA — nhiều repository tabs, 2026-09-23

## Automated

- `pnpm check`: 0 errors, 0 warnings; lint pass; 65 unit tests / 15 files pass.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`: 118 pass. Clippy `-D warnings` và fmt check pass.
- Vite production build pass. `pnpm tauri build` pass: release executable, `.deb`, `.rpm`, `.AppImage`; xem `native-build.txt` và checksums. Chưa install/clean-install hoặc chạy native UI flow trong task này.
- Regression tests dùng disposable repos: canonical subfolder reuse, multiple repo close isolation, stable worktree key qua close/reopen, linked-worktree independent HEAD/shared queue, busy close refusal. Browser mock factory tests tách index/branches/snapshot và chặn path token khác repo.

## Browser interactions

Vite `127.0.0.1:1422`, in-app browser, rõ nhãn Demo data. Các thay đổi stage chỉ ở mock memory; không dùng repo người dùng làm mutation fixture. Đã xem screenshot ở 1440×900 và 1280×800, không lưu file screenshot. Đã xem `example.webp` và wireframe `docs/design/01-working-changes.svg`; giữ palette/density/ba panel, thêm tab strip trên cùng.

1. Mở GitDock demo, nhập “Draft for GitDock”; mở website, input ban đầu trống, nhập “Draft for website”. Stage `new notes.txt` ở website → Staged 3; trở lại GitDock vẫn Unstaged 2, file đó chưa staged.
2. Mở merge commit/file ở GitDock, chuyển website thấy draft website; quay lại GitDock vẫn main-panel diff parent 1. Search “Nguyen” ở GitDock; website query trống, quay lại query GitDock giữ nguyên.
3. Ctrl+Tab chọn website (aria-selected=true); Ctrl+W đóng nó. Mở website từ Recent khôi phục draft. Chọn GitDock qua picker “Open tab” không thêm tab: vẫn 2.
4. Picker filter “missing-project” hiện no-match; Escape đóng picker. Tạo tổng 9 tabs @1280×800: tab strip 1566px scroll / 1074px viewport, pageWidth=viewport=1280, chỉ 1 visible tabpanel. Không tràn ngang toàn app; footer commit vẫn ở đáy inspector.
5. Sau reload preview, mở lại GitDock đọc đúng “Draft for GitDock”. Mở website rồi đóng GitDock background: website vẫn active. Đóng website cuối cùng về Welcome. Console warnings/errors: 0 tại cuối QA.

## Chưa kiểm chứng

Native UI flow và native multi-folder picker **blocked/not run**: không có native computer automation. Rust tests thực thi Git thật trong repo tạm, browser chỉ chứng minh frontend demo. Chưa nghiệm thu app-session restoration (chưa triển khai), native memory/frame profiling, hoặc package clean-install trong task này.
