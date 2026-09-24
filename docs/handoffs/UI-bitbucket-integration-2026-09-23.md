# Handoff — Bitbucket Cloud integration

- Ngày: 2026-09-23. User yêu cầu integration vì Push vẫn `AUTH_REQUIRED`.
- Phạm vi: Bitbucket Cloud HTTPS bằng API token scoped; SSH remote tiếp tục dùng SSH agent. Không thêm OAuth consumer/client secret, không hỗ trợ Bitbucket Data Center trong flow này.

## Root cause và quyết định

1. Bitbucket Cloud không còn chấp nhận App Password: không tạo mới từ 09/09/2025 và toàn bộ App Password cũ bị vô hiệu 09/06/2026. [Atlassian notice](https://support.atlassian.com/bitbucket-cloud/docs/revoke-an-app-password/).
2. Git push bằng API token cần `read:repository:bitbucket` và `write:repository:bitbucket`. GitDock dùng static username `x-bitbucket-api-token-auth` khi remote chưa có username, theo [Atlassian API-token guide](https://support.atlassian.com/bitbucket-cloud/docs/using-api-tokens/).
3. OAuth không được chọn vì desktop build chưa có registered Bitbucket consumer/client configuration. API token giải quyết HTTPS Git ngay, không thêm cloud backend vào sản phẩm.

## Implementation

1. Typed `bitbucket_connect` kiểm tra requestId, trust, repo version, remote name, URL host chính xác `bitbucket.org` và configured Git credential helper.
2. Secret-bearing request không implement `Debug`. Token tối đa 4096 bytes, reject control characters, chỉ feed qua stdin vào `git credential approve`; response/log không chứa token.
3. Credential context gồm HTTPS host/path/username. Fetch/Pull/Push tự prepend fixed Git `-c credential.username=...`; token không nằm trong argv/env/URL.
4. UI hiện **Bitbucket auth** cho remote HTTPS Bitbucket. Khi `AUTH_REQUIRED`, banner có **Connect Bitbucket**. Modal giải thích deprecation, required scopes, link tạo token, password input và **Save & retry push**.
5. Save thành công chỉ auto-start push khi chính nút ghi **Save & retry push** được user click. Token được xóa khỏi Svelte state khi đóng/thành công.

## Kiểm chứng và giới hạn

- `cargo test`: 122/122 pass. Test helper cô lập tạo local repo trong `/tmp`, reset helper chain và round-trip dummy token qua stdin; không chạm helper/credential thật.
- `pnpm test:unit`: 77/77 pass; `pnpm check` 0 errors/0 warnings; lint pass.
- Browser fixture @1280×720: Bitbucket banner, modal, scope chips, autofocus, password masking, enabled submit và callback 0→1 pass; console error/warning rỗng. [QA](../evidence/UI-bitbucket-integration-2026-09-23/qa.md).
- Final `pnpm tauri build`: exit 0; executable và ba Linux bundles được tạo lại. [Artifact checksums](../evidence/UI-bitbucket-integration-2026-09-23/sha256.txt), [source checksums](../evidence/UI-bitbucket-integration-2026-09-23/source-sha256.txt).
- Real Bitbucket push chưa chạy vì không có user API token/remote fixture. Build không đọc, ghi hoặc thay credential thật trong phiên.
- Repo source chưa commit/push/publish; worktree vẫn untracked trên `main`.

Thao tác tiếp theo (ước lượng 2 phút): tạo API token scoped, mở binary mới, chọn **Connect Bitbucket**, paste token và chọn **Save & retry push**.
