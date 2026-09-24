# Working changes + hunk actions QA — 2026-09-23

## Browser UI @1280×800

1. Inspector không còn checkbox và text header “Working tree”; hiển thị changed count/branch + nút Refresh.
2. Unstaged row có Discard + Stage; staged row chỉ có Unstage. Right-click unstaged row có Open diff, Stage file, Discard file changes và Copy path.
3. Worktree diff có Stage hunk + Discard hunk trên hunk header; action giữ trong main panel có horizontal scroll.
4. Discard hunk mở modal target-specific, focus Cancel. Escape đóng modal và giữ diff mở sau regression fix.
5. Stage hunk hoàn tất và reload diff. Discard untracked demo file hoàn tất, đóng diff, changed count 3→2 và file biến mất.
6. Console browser: 0 errors / 0 warnings.

## Automated gates

- `pnpm check`: 0 errors / 0 warnings.
- `pnpm lint`: pass.
- `pnpm test:unit`: 77 tests / 19 files pass.
- `cargo test`: 124 tests pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo fmt --all -- --check`: pass.
- `pnpm build`: pass, 188 modules.
- `pnpm tauri build`: executable, `.deb`, `.rpm` pass; AppImage sandbox run failed at linuxdeploy.
- `pnpm tauri build -- --bundles appimage` with network: pass.

## Git mutation cases

- Stage first of two distant hunks: cached diff contains only first hunk.
- Discard remaining hunk: working file restores only those lines; staged hunk remains.
- Discard tracked file restores index version.
- Discard untracked `untracked*.txt` deletes exact literal file and preserves neighbor.
- Content changed after confirmation: operation fails closed and preserves file.
- Broken untracked symlink can be confirmed and deleted without following its target.

Native UI interaction on a real user repository was not run. All mutation tests use disposable repositories under `/tmp`.
