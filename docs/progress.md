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
