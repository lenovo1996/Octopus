# Progress

## Fix remote checkout opening the Branches dialog (2026-09-24)

- Checking out a remote branch with a local twin (for example, `origin/main` when `main` already exists) opened the Branches dialog instead of switching. It now checks out the local twin directly, matching `git switch` DWIM behavior, through the new `resolveCheckoutTarget` helper in `branch-menu.ts`.
- CDP demo verification: checking out `origin/main` no longer opens Branches and follows the correct switch flow (`Stash and switch` appears because the demo worktree is dirty). A new test was added to `branch-menu.test.ts`; the complete unit suite passes 129/129 and lint is clean.

## Stage/unstage theo line (2026-09-24)

- Diff unstaged: nút "+" trên từng dòng add/delete để stage đúng dòng đó; diff staged ("Staged · HEAD → index"): nút "−" để unstage đúng dòng. Nút hiện khi hover/focus, disable kèm lý do khi hunk có marker no-trailing-newline (giữ hunk-level, fail closed).
- Backend: `select_patch_lines` (ordinal đếm dòng +/-; dropped `-` thành context, dropped `+` bỏ) + `diff_lines_stage` (`apply --cached`) / `diff_lines_unstage` (`apply --cached --reverse` trên staged patch). Ordinal rebuild từ bytes mới nhất trước mỗi lần apply nên token cũ fail closed.
- Tests: 3 unit `select_patch_lines` + integration temp-repo (stage đúng 1 dòng, stale hunk STALE_STATE, unstage đúng 1 dòng, worktree nguyên bytes); FE `diff-lines.test.ts` cho rule ordinal. Rust 149/149, clippy/fmt sạch; FE 130/130, lint + check sạch.
- CDP demo: hunk có marker thì nút disable đúng title; hunk-2 stage/unstage click không lỗi, diff vẫn mở.
- Build line-stage: .deb + .rpm mới 11:52 (có tính năng); AppImage fail Text file busy do app cũ còn chạy — user tắt app cũ rồi bundle lại AppImage sau.

## Fix GIT_ERROR unstage lines (2026-09-24)

- Nguyên nhân: unstage dùng `apply --cached --reverse` trên sub-patch cắt từ staged diff — old side của patch đảo ngược mô tả blob không tồn tại ở đâu nên git luôn rớt (trừ case xóa thuần). Fix: `select_patch_lines_staged` dựng patch forward khớp index thật (dòng giữ đảo dấu, dropped `+` thành context, dropped `-` bỏ), apply `--cached` thường.
- Thêm 2 edge: file mới stage + unstage từng phần (viết lại prelude `--- a/...`), unstage hết dòng → gỡ entry bằng `restore --staged` (giống whole-file unstage; `rm --cached` bị git từ chối khi worktree đã đổi); file đã xóa khỏi index thì từ chối line-level, dùng unstage cả file.
- Tests: 3 unit staged + integration temp-repo (mod + new-file partial/full). Rust 153/153, clippy/fmt sạch; FE không đổi (130/130, lint sạch). Build đủ 3 bundle.

## Fix PR UNSUPPORTED với remote có username (2026-09-24)

- Remote `https://user@bitbucket.org/...` bị nhánh parse SSH short-form chặn trước (userinfo chứa `/` nên return None), nhánh https strip userinfo phía sau thành dead code. Fix: parse scheme trước, SSH chỉ khi không có `://`.
- Tests: thêm case userinfo bitbucket/github vào test detect sẵn có (fail trước, pass sau). Rust 153/153, clippy sạch. Build đủ 3 bundle.

## Drag-and-drop repository tab ordering (2026-09-24)

- Repository tabs support HTML5 drag and drop with before/after indicators, plus "Move tab left/right" context-menu actions for keyboard users. The existing `workspacesSave` effect persists the new order.
- Added the `reorderTabs` helper in `tabs.ts` and coverage in `tab-order.test.ts` (failed before the implementation, passed afterward). Full unit suite: 132/132; lint and type checks passed.
- Verified with two tabs in the CDP demo: dragging the first tab past the second reversed their order correctly.

## Stabilize Git identity in CI tests (2026-09-25)

- GitHub Actions reached the Rust test step but five fixtures failed because the runner had no global Git author or committer identity.
- Temporary commit-action repositories now configure their own local identity, and History test commands provide an explicit one-shot identity. The fixtures no longer depend on a developer machine's global Git config.
- Verified the affected modules and the full 153-test Rust suite with `GIT_CONFIG_GLOBAL=/dev/null`; formatting and Clippy checks also passed.
