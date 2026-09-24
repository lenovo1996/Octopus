# Handoff — Push authentication recovery

- Ngày: 2026-09-23. User báo `Push failed (AUTH_REQUIRED)`.
- Phạm vi: giữ nguyên credential policy MVP; sửa structured error transport và UX recovery. Không lưu token, đọc credential file, đổi Git config hoặc tự retry push.

## Root cause và fix

1. `network_error` đã tạo `AppError`, nhưng `execute_remote` thu gọn failure thành string code. `RepoRegistry::job_finish` chỉ lưu `errorCode`, nên UI mất message/recovery và chỉ hiện `push failed (AUTH_REQUIRED)`.
2. `JobOutcome::Failed` nay mang `AppError`; terminal `OperationRecord` có `error` cùng compatibility `errorCode`. Auth dùng recovery `authenticate`. Spawn/timeout/remote errors cũng có safe message; raw stderr không đi qua IPC.
3. UI dùng URL đã redact để chọn hướng dẫn: HTTPS yêu cầu refresh access token trong credential helper; SSH yêu cầu start/unlock agent và xác nhận key có quyền repo. Banner giữ error code và có Retry theo đúng operation kind.
4. Retry chỉ chạy sau click; push không auto-replay. Existing `GIT_TERMINAL_PROMPT=0`, credential helper/SSH agent, host-key checks và trust gate không đổi.

## Kiểm chứng và giới hạn

- `pnpm check`: 0 errors/0 warnings; lint pass; 75 tests/18 files pass; production build pass.
- `cargo test`: 119/119 pass; clippy `-D warnings` và fmt check pass. Test mới tái hiện stderr HTTPS auth, xác nhận `AUTH_REQUIRED`, recovery `authenticate` và message không lộ hostname fixture.
- Browser fixture @1280×720: code, HTTPS guidance và Retry push hiện đầy đủ; click tăng retry count; console error/warning rỗng. [QA](../evidence/UI-push-auth-recovery-2026-09-23/qa.md).
- Final `pnpm tauri build`: exit 0; executable và ba Linux bundles được tạo lại. [Artifact checksums](../evidence/UI-push-auth-recovery-2026-09-23/sha256.txt), [source checksums](../evidence/UI-push-auth-recovery-2026-09-23/source-sha256.txt).
- Máy build báo `credential.helper=store`, `SSH_AUTH_SOCK` set. Không đọc credential/key. Native push thật chưa chạy vì không có remote/credential đích được user chỉ định.
- Source chưa commit/push/publish; worktree vẫn untracked trên `main`.

Thao tác tiếp theo (ước lượng 2 phút): refresh token hoặc unlock SSH key, mở binary mới, Push và kiểm tra operation chuyển sang `succeeded`.
