# Handoff — nhiều repository tabs

- Ngày: 2026-09-23. T17, yêu cầu trực tiếp của user: “design thêm tính năng mở nhiều repo”.
- Implementation + automated checks + browser QA + native packaging hoàn tất. Native window workflow **blocked/not run** vì phiên chỉ có browser UI automation.
- Đây là ngoại lệ user yêu cầu cho multi-repo tabs trước M4; không triển khai các mục P2 khác. Giữ stack/dependencies và bố cục GitDock.

## Thay đổi

1. `App.svelte` sở hữu tab lifecycle, picker, native multi-folder Open và Init. `RepositoryWorkspace.svelte` tách từ shell cũ, mount riêng theo repoId. Mỗi tab có state/callbacks/operations riêng; tab ẩn không nhận phím tắt foreground. Destroy dọn listeners, search timer, polling và lưu draft.
2. `RepositoryTabs.svelte`: tên repo/branch, trạng thái changed/draft/busy/error, active border, ×, horizontal overflow và keyboard navigation. `RepositoryPicker.svelte`: Recent search, Open tab cho repo đã mở, browse/init, loading/error/empty và focus trap. Các action Open/Init/Close đưa vào Menu thay cho topbar bị lặp.
3. Backend registry trước đây dedup theo common dir nên linked worktree thứ hai nhận nhầm session của worktree đầu. Nay dedup theo canonical worktree root + git dir; queue/trust vẫn chung common dir. `RepoSnapshot.workspaceKey` ổn định từ exact worktree path bytes dùng riêng cho UI persistence. Recent entries mới không gộp hai worktrees.
4. Draft `gitdock.drafts.v2` lưu theo workspaceKey, giữ subject/body qua close/reopen/restart. Store v1 không bị xóa; chỉ fallback nếu key session cũ khớp. Mock adapter được tách instance riêng mỗi repo để demo không trộn branches/index/snapshot. Summary input ID cũng riêng từng workspace.

## Hành vi đã kiểm tra

| Check | Kết quả |
|---|---|
| `pnpm check` | 0 errors / 0 warnings |
| `pnpm lint` | pass |
| `pnpm test:unit` | 65 tests / 15 files pass |
| Rust tests | 118 pass |
| Rust clippy `-D warnings` / fmt check | pass |
| `pnpm build` | pass |
| `pnpm tauri build` | pass; release executable + `.deb`/`.rpm`/`.AppImage` |
| Browser demo @1440×900, @1280×800 | pass trong phạm vi bên dưới |
| Native multi-folder dialog + real-repo UI workflow | **blocked/not run**; backend fixture tests không thay thế native E2E |

Browser: hai repo giữ draft khác nhau; stage ở website không ảnh hưởng GitDock; search và commit diff giữ riêng khi switch. Ctrl+Tab chọn tab tiếp theo; Ctrl+W đóng active tab, mở lại khôi phục draft. Open cùng repo chọn tab đã có; đóng tab background giữ active tab, đóng tab cuối về Welcome. Reload rồi mở repo khôi phục draft. Chín tabs ở 1280px có tab scrollWidth 1566px trong clientWidth 1074px; pageWidth vẫn 1280px và chỉ một tabpanel visible. Console không warnings/errors lúc kiểm tra.

Backend regression tests: repo/subfolder dùng lại session, hai repo đóng độc lập và giữ stable draft key, linked worktrees HEAD/key/session khác nhau nhưng cùng queue, close busy bị chặn mà vẫn cho đóng repo idle. Frontend mock tests chứng minh index/branch isolation và từ chối token của repo khác.

Evidence: [QA](../evidence/UI-multi-repo-2026-09-23/qa.md), [Rust tests](../evidence/UI-multi-repo-2026-09-23/rust-tests.txt), [native build](../evidence/UI-multi-repo-2026-09-23/native-build.txt), [artifact checksums](../evidence/UI-multi-repo-2026-09-23/sha256.txt), [source checksums](../evidence/UI-multi-repo-2026-09-23/source-sha256.txt). Source vẫn untracked trên `main` chưa có commit. Không tự commit/push/install/publish.

Bản mới: `src-tauri/target/release/gitdock`, `bundle/deb/GitDock_0.1.0_amd64.deb`, `bundle/rpm/GitDock-0.1.0-1.x86_64.rpm`, `bundle/appimage/GitDock_0.1.0_amd64.AppImage` (các đường dẫn bundle tính từ `src-tauri/target/release/`). Phải mở binary mới để frontend và DTO `workspaceKey` khớp nhau.

## Giới hạn và bước nghiệm thu

- Các tab chỉ tồn tại trong app session; không tự mở lại sau restart. Draft giữ bền theo worktree key khi mở lại repo.
- Worktree/index diff đóng khi refresh listing lúc active lại vì pathId cũ có thể hết hạn; commit diff giữ nguyên. Snapshot/status được đọc lại, cache UI không dùng thay Git truth để mutation.
- Native acceptance còn thiếu: multi-folder dialog, nhiều repo/worktrees thật trong window và operations nền. T15/T16 và M1–M4 không được nâng passed bằng browser QA.
- Thao tác tiếp theo (ước lượng 2 phút): mở executable mới, mở repo A → **+ Open repo** → repo B → nhập draft riêng → Ctrl+Tab → kiểm tra draft của A.
