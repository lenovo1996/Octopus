# QA — native diff và inspector, 2026-09-23

## Automated checks

- Frontend: `pnpm check` 0 errors/0 warnings, lint pass, 63 unit tests/14 files pass, production build pass.
- Backend: 115 tests pass, clippy `-D warnings` và fmt check pass; xem `rust-tests.txt`.
- Trước fix, regression test deserialize `DiffReadRequest` lỗi: `unknown variant 'worktree', expected one of 'Worktree', 'Index', 'Commit'`.
- Sau fix, 4 tests của commands/diff pass: frontend JSON sources, staged/unstaged nội dung riêng trên disposable repo, commit JSON diff và trust/token guards.
- Tests mới frontend bao phủ bounds/persistence, exact ref tips/local/remote/tag, selection theo source + opaque token, string transport error recovery không lộ raw secrets.
- Release compile thành công trong 2m38s; `.deb`/`.rpm` tạo thành công. AppImage thất bại ở download runtime khi chạy sandbox; bundling lại với quyền mạng đã pass, xem `appimage-build.txt`. `dpkg-deb --info` đọc được package `git-dock` 0.1.0 amd64. Không cài đặt artifacts vào hệ thống.

## Browser demo — đã thao tác trực tiếp

Local Vite tại `127.0.0.1:1422`, nhãn **Demo data**, in-app browser. Đã xem screenshot tại 1440×900 và 1280×800; không lưu screenshot file. Mutation browser chỉ thay mock memory, không dùng repository người dùng làm fixture.

1. Resize Graph bằng chuột: 84 → 134px; Subject bằng ArrowRight → 407px; Author → 148px. Reload, mở lại demo: Branch/Graph/Subject/Author vẫn là **150/134/407/148px**. Double-click Graph/Subject và Home Author reset; tại 1280×800 đo **150/84/289/140px**. Header/rows thẳng hàng, Author có trong bảng, overflow nằm trong history.
2. Branch bên trái graph: cùng commit hiển thị `local main` và `origin main`; nhánh `feature/ui` ở commit của nó. Đường graph và row/node alignment giữ đúng sau resize.
3. Chọn checkbox Unstaged `src/app/App.svelte` không chọn checkbox Staged của cùng file. Action nhóm đổi thành `Stage 1`; click làm Unstaged 2 → 1, chỉ còn `new notes.txt`. Sau reload reset fixture, Unstage riêng `src/lib/ipc/types.ts` làm Staged 2 → 1, Unstaged 2 → 3.
4. Commit details: Copy ID báo Copied; filter path không có kết quả hiện empty message; xóa filter khôi phục files. Click file mở main diff; chọn Parent 2 đóng diff cũ; click file lại đọc diff có nhãn Parent 2. Escape trả graph; View parent điều hướng sang `Add inspector conflict panel`/B. Tran.
5. Working changes ở 1280×800 có file groups cuộn độc lập, commit editor dưới cùng, branch/count/refresh và lý do commit disabled. Commit details giữ Back to working changes ở footer. Browser console không có warnings/errors tại thời điểm kiểm tra.

## Giới hạn evidence

Native visual/workflow gate **blocked/not run**: session không có native UI control; không kết luận native E2E pass từ browser demo. Backend tests chứng minh JSON boundary và Git engine đọc diff trên repo tạm. Full native workflow và frame/memory gates của T15/T16 vẫn chưa nghiệm thu.
