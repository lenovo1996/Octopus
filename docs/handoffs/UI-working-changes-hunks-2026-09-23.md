# Handoff — Working changes discard và partial hunk actions

- Ngày: 2026-09-23.
- Yêu cầu: bỏ checkbox và header “Working tree”, giữ Refresh, thêm discard file, Stage hunk, Discard hunk và cải thiện UX/UI.

## Hành vi hoàn tất

1. Working changes dùng direct actions: Unstaged có Discard + Stage, Staged có Unstage; group header giữ Stage/Unstage all. Context menu có Discard file changes. Thanh đầu chỉ còn changed count, branch và Refresh có label.
2. Click file vẫn mở diff ở main panel. Mỗi worktree text hunk có Stage hunk và Discard hunk; untracked/binary/truncated diff giải thích vì sao partial action không khả dụng.
3. Discard file/hunk luôn qua modal destructive action, focus mặc định Cancel. Escape chỉ đóng modal, không đóng diff phía sau.
4. Backend nhận `pathId + hunkId`, tự rebuild raw patch và feed stdin vào `git apply`; frontend không gửi patch text hoặc argv. Stage hunk dùng `--cached`; discard hunk dùng `--reverse`.
5. Discard tracked file restore worktree từ index. Untracked dùng `git clean -f` với một exact literal path, không directory/ignored/all. Confirmation bind repo version + exact target; file bind fingerprint, hunk bind raw-byte hunkId.

## Kiểm chứng

- Frontend: 77 tests / 19 files; Svelte check 0 errors/0 warnings; ESLint và production build pass.
- Rust: 124 tests; Clippy `-D warnings` và fmt pass. Mutation tests chạy trong repo tạm, gồm 2 hunks, tracked/untracked, magic `*`, broken symlink và stale confirmation.
- Browser demo 1280×800: direct actions, context menu, hunk toolbar, Cancel focus, Escape isolation, Stage hunk refresh, Discard untracked count 3→2 và console sạch.
- Native release: executable + `.deb` + `.rpm` + `.AppImage` đã build lại. Sandbox fail tại linuxdeploy AppImage; rerun bundler AppImage với network pass.

## Giới hạn có chủ đích

- Partial action chỉ cho tracked text worktree diff chưa truncated. Untracked file dùng whole-file Stage/Discard.
- Không có undo sau discard đã xác nhận; GitDock không tạo backup riêng.
- Native window interaction trên repo thật chưa chạy trong phiên; Rust integration tests kiểm chứng Git mutation bằng repo tạm.

Thao tác tiếp theo (dưới 2 phút): chạy binary mới, sửa hai vùng xa nhau trong một tracked text file, mở diff và thử **Stage hunk** cho một vùng.
