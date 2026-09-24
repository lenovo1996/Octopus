# Push authentication recovery QA — 2026-09-23

Phạm vi browser fixture: `tests/ui/sync-error.html` @1280×720. Đây là UI/contract QA, không phải push tới remote thật.

## Hành vi đã kiểm tra

1. Banner persistent hiện `AUTH_REQUIRED` và câu: “Push authentication was rejected. Refresh the access token in your Git credential helper and verify repository access, then retry.”
2. Banner không render remote URL, raw stderr, token hoặc credential content.
3. `Retry push` là button focusable; click tăng fixture retry count 0 → 1, chứng minh callback được gọi đúng.
4. Console error/warning rỗng.

## Automated checks

- `pnpm check`: 0 errors / 0 warnings.
- `pnpm lint`: pass.
- `pnpm test:unit`: 75 tests / 18 files pass.
- `cargo test`: 119 tests pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo fmt --all -- --check`: pass.
- `pnpm build`: pass.
- `pnpm tauri build`: exit 0; executable, `.deb`, `.rpm` và `.AppImage` được tạo lại. Xem [artifact checksums](sha256.txt) và [source checksums](source-sha256.txt).
- Native real-remote push: **blocked/not run**; user repo/remote/credential không được dùng làm fixture.
