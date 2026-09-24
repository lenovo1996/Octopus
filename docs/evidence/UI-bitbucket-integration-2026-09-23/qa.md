# Bitbucket integration QA — 2026-09-23

Phạm vi browser fixture: `tests/ui/bitbucket-auth.html` @1280×720. Đây là UI/contract QA; không gửi token hay push tới Bitbucket thật.

## Hành vi đã kiểm tra

1. `AUTH_REQUIRED` cho `https://bitbucket.org/...` hiện cả **Connect Bitbucket** và **Retry push**; toolbar có **Bitbucket auth**.
2. Modal ghi rõ App Password ngừng hoạt động, required permissions Repository Read + Write và remote URL đã redact.
3. API token input là password field, autofocus; accessibility tree không trả giá trị dummy. Submit disabled khi rỗng và enabled sau input.
4. **Save & retry push** đóng modal, xóa field state trong fixture và tăng callback count 0 → 1. Console error/warning rỗng.

## Automated checks

- `pnpm check`: 0 errors / 0 warnings.
- `pnpm lint`: pass.
- `pnpm test:unit`: 77 tests / 19 files pass.
- `cargo test`: 122 tests pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo fmt --all -- --check`: pass.
- `pnpm build`: pass, 187 modules.
- `pnpm tauri build`: exit 0; executable, `.deb`, `.rpm` và `.AppImage` được tạo lại. Xem [artifact checksums](sha256.txt) và [source checksums](source-sha256.txt).
- Native real-remote push: **blocked/not run**; không có Bitbucket API token/repo fixture của user.
