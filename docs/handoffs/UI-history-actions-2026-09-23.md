# Handoff — History actions (T18)

- Ngày: 2026-09-23. Yêu cầu trực tiếp của user: build 16 tính năng context menu đang `Planned`.
- Cả 16 action commit menu nay có typed IPC command + modal riêng; không còn mục `Planned` nào trong commit menu. `Create branch here…` vẫn dùng branches dialog.

## Thay đổi

1. Backend mới `src-tauri/src/commands/commit_actions.rs`: 16 commands trả `RepoSnapshot`, chung gate WriteContext + trust + version, queue theo common-dir, bump version + invalidate history. Đăng ký trong `commands/mod.rs` và `lib.rs`.
2. An toàn: OID validate theo object format + `cat-file -t commit`; clean-worktree cho checkout/cherry-pick/revert/merge/rebase; chặn merge/rebase đang dở; không force push, không `branch -D`. Reset-hard/rebase/split/interactive dùng token `confirmation_prepare` (`history_reset_hard`, `history_rebase`, `history_split`, `history_rebase_interactive`); hard reset modal yêu cầu gõ lại short OID.
3. Semantics: cherry-pick/revert `--no-commit` để review staged result; `merge_commit` ghi app-origin record nên `merge_complete`/`merge_abort` tái dùng được; amend-family (reword/modify/edit-author/split) HEAD-scoped, commit cũ hơn đi qua interactive rebase; interactive rebase chạy scripted `GIT_SEQUENCE_EDITOR` với plan pick/reword/squash/fixup/drop, reword qua `exec amend -F` file (không argv), backend từ chối plan không bao phủ đúng range `parent..HEAD`.
4. Frontend: `commit-menu.ts` bật cả 17 action; `commit-action-forms.ts` giữ metadata thuần (đã có unit test); `CommitActionModal.svelte` render form/plan/confirm-summary; `HistoryPane` thêm `onCommitAction`; `RepositoryWorkspace` mở modal, xin token, gọi adapter, refresh history/status/snapshot cả khi lỗi. `real.ts` + `mock.ts` đủ 16 methods.
5. Docs: IPC contracts §4 thêm 16 dòng; Git engine §8 cập nhật trạng thái T18; progress.md thêm hàng T18.

## Kiểm chứng và giới hạn

- `cargo test`: **132/132 pass** (8 tests mới trên repo `/tmp`: checkout detach, tag trùng, cherry-pick stage, cherry-pick conflict, resets + token, reword HEAD-scope, interactive drop+reword, merge conflict giữ app flow).
- `cargo clippy -- -D warnings`, `cargo fmt --check` sạch. `pnpm check` 0 errors, `pnpm lint` sạch, `pnpm test:unit` **81/81 pass** (20 files; 2 tests mới cho forms + menu), `pnpm build` pass, `pnpm tauri build --no-bundle` ra binary `src-tauri/target/release/gitdock`.
- Native real-repo workflow **blocked/not run**: chưa mở app desktop trên repo thật để chuột phải từng action trong phiên này; M1–M4 không đổi.
- Không commit/push/install/publish. Source vẫn untracked trên `main`.

Thao tác tiếp theo (ước lượng 2 phút): mở binary mới trên repo tạm, chuột phải commit thử Tag, Cherry-pick (kiểm tra staged chưa commit) và Reset hard (kiểm tra gõ lại OID + token).
