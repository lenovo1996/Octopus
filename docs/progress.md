# Implementation progress

**Current phase:** M4 stabilization và mở rộng T17 theo yêu cầu user; native acceptance còn thiếu. **Current work:** Bitbucket Cloud API-token integration. **Last updated:** 2026-09-23.

| Task | Milestone | Status | Evidence / blocker |
|---|---|---|---|
| T01 | M0 | done | Scaffold + preflight + native smoke pass; xem docs/handoffs/T01.md và docs/evidence/T01/ |
| T02 | M0 | done | Shell 3 cột + 3 states mock pass 1440×900/1280×800; xem docs/handoffs/T02.md, docs/evidence/T02/ |
| T03 | M1 | done | Runner + discovery/open/init/recent/trust pass 15 Rust tests + native smoke; xem docs/handoffs/T03.md, docs/evidence/T03/ |
| T04 | M1 | done | Backend refs/history/search/details 28/28 tests; FE DTOs + nối UI sang T05; xem docs/handoffs/T04-backend.md |
| T05 | M1 | in_review | Graph + history integration xong (tests + headless DOM); native visual chờ user confirm; xem docs/handoffs/T05.md |
| T06 | M1 | in_review | Scope/search/keyboard xong (25/25 tests, check/lint/build sạch, headless DOM pass); native visual chờ user confirm chung với T05; xem docs/handoffs/T06.md |
| T07 | M2 | done | repo_status + tokens + counts + coordinator + FE Working changes; 48 Rust + 29 FE tests; xem docs/handoffs/T07.md (native visual chờ user chung gate T05/T06) |
| T08 | M2 | done | diff_read + tokens + unified viewer; 59 Rust + 31 FE tests; xem docs/handoffs/T08.md (native visual chờ user chung gate) |
| T09 | M2 | done | index_stage/unstage + WriteContext + FE buttons; 63 Rust + 32 FE tests; xem docs/handoffs/T09.md (native visual chờ user chung gate) |
| T10 | M2 | done | commit/identity/branch + dialogs; 73 Rust + 35 FE tests; xem docs/handoffs/T10.md (native visual chờ user chung gate) |
| T11 | M3 | done | remote_status + jobs fetch/pull/push + operation_log + toolbar FE; 83 Rust + 39 FE tests; xem docs/handoffs/T11.md (native visual + native sync chờ user chung gate; clone/remote_list để tương lai) |
| T12 | M3 | done | stash list/save/apply/pop + OID identity + StashModal; 88 Rust + 42 FE tests; xem docs/handoffs/T12.md (native visual chờ user chung gate) |
| T13 | M3 | done | merge --no-ff --no-commit + conflict stages/preview/accept/resolve/complete/abort + dialogs; 99 Rust + 46 FE tests; xem docs/handoffs/T13.md (native visual chờ user chung gate) |
| T14 | M3 | done | settings v1/migrate + font/menu/help/drafts/widths/shortcut-scope + redaction corpus fix; 108 Rust + 49 FE tests; xem docs/handoffs/T14.md (native visual chờ user chung gate) |
| T15 | M4 | done (partial) | gates + perf pass + native launch smoke + QA mapping; full E2E/frame/memory blocked; xem docs/handoffs/T15.md |
| T16 | M4 | done (partial) | .deb/.rpm/.AppImage + sha256 + runbook + release notes; clean-install + MIT đã ghi nhận ở cuối T16 handoff; full native E2E còn thiếu; xem docs/handoffs/T16.md |
| T17 | User extension | in_review | Multi-repo tabs + picker + independent workspaces, 65 FE/118 Rust tests pass, browser QA pass; native workflow blocked/not run; xem docs/handoffs/UI-multi-repo-2026-09-23.md |
| T18 | User extension | in_review | 16 commit-menu actions có typed IPC + modal (132 Rust + 81 FE tests pass, check/lint/clippy/fmt/build sạch, `tauri build --no-bundle` pass); native real-repo workflow blocked/not run; xem docs/handoffs/UI-history-actions-2026-09-23.md |
| T19 | User extension | in_review | Lưu/khôi phục workspaces đã mở (135 Rust + 84 FE tests pass, check/lint/clippy/fmt/build sạch, `tauri build --no-bundle` pass); native restart workflow blocked/not run; xem docs/handoffs/UI-workspace-restore-2026-09-23.md |

## Milestone gates

| Gate | State | Evidence |
|---|---|---|
| M0 — native scaffold + shell | Passed 2026-09-22 | T01+T02 done; native window + 3 states @1440×900/1280×800, xem docs/evidence/T01 + T02 |
| M1 — read-only real repository | Not run | — |
| M2 — local workflow | Not run | — |
| M3 — Linux MVP features | Not run | — |
| M4 — release candidate | Not run | — |

## Decisions carried forward

- User xác nhận Linux trước, rồi Windows/macOS.
- Rust + Svelte và bố cục theo example.webp là yêu cầu gốc; Tauri 2 là baseline đề xuất.
- App đã triển khai Rust/Tauri 2/Svelte 5; checks và evidence cụ thể ở từng handoff. Không đồng nhất browser demo/BE tests với native E2E.
- Kiểm tra lại 2026-09-23: Git worktree hợp lệ trên `main` nhưng chưa có commit; source hiện là untracked files. Không tự commit/push hoặc ghi đè metadata Git.

## Session handoff

Handoff hiện tại: [Bitbucket integration](handoffs/UI-bitbucket-integration-2026-09-23.md), sau [push auth recovery](handoffs/UI-push-auth-recovery-2026-09-23.md) và [context menu](handoffs/UI-context-menu-2026-09-23.md). Bước tiếp theo là nhập scoped API token rồi thử Push trên repo Bitbucket thật. T15/T16 vẫn partial vì full native workflows và frame/memory profiling chưa được kiểm chứng.

## Kiểm tra bộ tài liệu (không phải kiểm thử ứng dụng)

- Đã kiểm tra các local Markdown links không trỏ tới file thiếu, code fences đóng đủ, đủ 16 task IDs và 25 requirement IDs.
- Ba SVG đã parse XML và render để xem trực tiếp; đã sửa lỗi màu chữ/font xuất hiện ở renderer đầu tiên.
- Đã đo 7 cặp text/background tokens chính, contrast thấp nhất 5.87:1; đây là kiểm tra palette, chưa phải accessibility audit app.
- Phần này chỉ ghi kiểm tra tài liệu ban đầu. Trạng thái build/Git tests hiện tại nằm trong các task và fix bên dưới; native milestone gates chưa được nghiệm thu đầy đủ.

## Bugfix History repo lớn (2026-09-22, ngoài backlog)

- Triệu chứng (user báo trên app thật): mở repo ~223k commits / 4201 refs → `History failed (GIT_ERROR): Failed to read history`.
- Root cause: `ensure_session` tải toàn bộ topo một lần (~23.6MB) vượt `MAX_OUTPUT_BYTES` 8MB của runner → `OutputLimit`. Đã tái hiện bằng `rev-list` tương đương trên repo lỗi (chỉ đọc, không ghi).
- Fix: `read_topology_capped` (`--max-count`, cap 20k rows + cờ truncated), `read_metadata` chia batch 2000 + feed stdin đồng thời (sửa luôn deadlock pipe khi oid list vượt pipe buffer — test mới từng treo suite), `HistorySession.truncated` → `HistoryPage.truncated` (chỉ ở trang cuối) / `SearchResults.incomplete`; UI hiện "showing newest N (older history omitted)".
- Tests mới: `topology_truncates_at_explicit_cap`, `metadata_batches_past_chunk_size`, `page_flags_truncation_only_at_cache_end`. Gates: BE 113 pass, clippy/fmt xanh; FE check/lint xanh, 49 tests pass; `pnpm tauri build` ra .deb/.rpm/.AppImage mới.
- Follow-up: `runner.run_with_stdin` còn pattern write-then-read tuần tự (payload hiện tại nhỏ nên chưa treo) — cân nhắc feed đồng thời khi payload lớn hơn.
- Chờ user restart app và mở lại repo lỗi để kiểm chứng.

## Bugfix merge-commit Details (2026-09-22, ngoài backlog)

- Triệu chứng (user báo trên app thật): click merge commit → `Details failed (INVALID_ARGUMENT): Merge commit requires an explicit parent selection`, panel kẹt vì dropdown chọn parent chỉ hiện sau khi details load xong.
- Root cause: `read_commit_files` (git/history.rs) và `diff_read` (commands/diff.rs) từ chối `parentIndex=null` ở merge commit, trái spec docs/04-ipc-contracts.md ("Parent index default 0 cho non-root; null chỉ hợp lệ cho root").
- Fix: default parent 0 cho mọi non-root (kể cả merge) ở cả 2 chỗ; UI giữ dropdown đổi parent như cũ.
- Test: cập nhật `merge_details_and_parent_selection` (None → ok, parent_index Some(0)). Gates: BE 113 pass, clippy/fmt xanh; không đổi FE.
- Chờ user restart app để kiểm chứng.

## Fix graph UI và main-panel diff (2026-09-23)

- Graph: không còn rail giả trước tip/sau root; cạnh first-parent/merge được biểu diễn rõ, khôi phục toàn bộ lane reservations khi qua page. Demo dùng đúng parent OIDs; merge marker theo số parents.
- UI: graph gutter tối thiểu 80px, cạnh cong, 32px row/node alignment, metadata ẩn theo chiều rộng panel, scroll ngang đồng bộ header, keyboard tự đưa row focus vào viewport. Fixture 10k commits/24 lanes chỉ render 26–27 rows.
- Diff: click file mở main panel; ×/Escape trả lại graph; giữ file list bên phải. Chọn staged/unstaged dùng exact pathId và source `index`/`worktree`. Request sequencing bảo vệ diff và commit details khỏi response cũ; đổi parent đóng diff.
- Gates đã chạy: `pnpm check` 0 errors/0 warnings; lint pass; **57/57 FE tests**; **113/113 Rust tests**; `pnpm build` và `pnpm tauri build --no-bundle` pass (binary `src-tauri/target/release/gitdock`). Browser demo visual/interactions ở 1280×800 và 1440×900 pass trong phạm vi fix.
- Native visual/workflow cho fix này **blocked/not run**: browser hiện có chỉ chứng minh demo; chưa có native UI automation trong phiên. Không đánh dấu M1–M4 passed từ evidence browser.
- Chi tiết thay đổi, build và checklist: [handoff](handoffs/UI-graph-diff-2026-09-23.md), [QA evidence](evidence/UI-graph-diff-2026-09-23/qa.md).

## Fix native diff, history columns và inspector UX (2026-09-23)

- Native diff: Rust `DiffTarget` thiếu enum-level `rename_all`, nhận `Worktree/Index/Commit` trong khi frontend gửi `worktree/index/commit`. Test JSON đã tái hiện lỗi trước fix; sửa Serde để khớp contract. Transport error dạng string có hướng dẫn restart an toàn, không hiển thị raw native payload.
- History: cột Branch nằm bên trái graph, nhãn phân biệt local/origin; Branch/Graph/Subject/Author kéo resize hoặc dùng phím mũi tên, double-click/Home để reset; lưu độ rộng sau reload. Graph giữ đủ chiều rộng lane; Author luôn có trong bảng, dùng scroll ngang khi thiếu chỗ.
- Working changes: file name/path rõ ràng, nhóm Unstaged/Staged có số lượng và action riêng; checkbox cùng file ở hai nguồn độc lập; stage/unstage từng file hoặc nhóm dùng exact pathId. Commit editor cố định ở dưới với lý do disabled, description tùy chọn và identity.
- Commit details: subject/OID/copy, ref labels, author/date, chọn và điều hướng parent, lọc changed files; click file vẫn mở main-panel diff có ×/Escape.
- Gates: **63/63 FE tests**, **115/115 Rust tests**, check/lint/clippy/fmt và frontend build pass. Browser demo 1280×800/1440×900 đã kiểm tra resize/persistence, staged selection, stage/unstage, copy/filter/parent/diff. Native visual/workflow **blocked/not run** vì không có native UI automation; không đổi trạng thái M1–M4.
- Evidence và bản build mới: [handoff](handoffs/UI-native-diff-inspector-2026-09-23.md), [QA](evidence/UI-native-diff-inspector-2026-09-23/qa.md).
- Đã rebuild executable, `.deb`, `.rpm` và `.AppImage`. AppImage ban đầu bị sandbox chặn download runtime; `tauri bundle --bundles appimage --verbose` chạy lại với quyền mạng đã pass. Không cài đặt hoặc publish artifacts.

## T17 — Multi-repository tabs (2026-09-23)

- User yêu cầu mở nhiều repo; scope được nâng từ P2 theo chỉ đạo trực tiếp, không mở thêm các mục P2 khác. ADR-010 được cập nhật.
- Tab strip có repo/branch/changed files/draft/busy/error và ×; picker Recent có filter, chọn tab đang mở, native multi-folder picker và Initialize. Workspace riêng giữ draft/search/scope/selection/commit diff, xử lý async và operations theo repo sở hữu.
- Backend dedup theo canonical worktree + git dir; linked worktrees không còn bị alias vào một session. Stable workspaceKey cho draft, recent key theo worktree; queue/trust vẫn dùng common dir. Close busy giữ session; close tab cuối về Welcome.
- Checks: 65 FE tests/15 files, 118 Rust tests, check/lint/clippy/fmt và production build pass. Browser QA hai repo và overflow chín tabs @1280×800/1440×900; close/reopen/reload draft, Ctrl+Tab/Ctrl+W, duplicate-open, search/diff isolation pass. Native multi-folder picker và real-repo window workflow **blocked/not run**.
- [Handoff](handoffs/UI-multi-repo-2026-09-23.md) và [QA evidence](evidence/UI-multi-repo-2026-09-23/qa.md). Không tự restore tabs khi restart; không commit/push/install/publish trong phiên.
- `pnpm tauri build` pass: executable và cả ba gói Linux đã rebuild với tính năng T17; checksums nằm trong evidence mới.

## Follow-up — Graph resize / horizontal scroll (2026-09-23)

- Đã tái hiện Graph 24 lanes bị khóa width tối thiểu 496px, không thu nhỏ được. Cột Graph nay resize 64–1600px độc lập lane count, giữ geometry nhờ thanh cuộn lane ngay dưới header.
- Main panel có scrollbar ngang ở đáy; sticky header và rows chung scroll container nên focus/scroll không làm lệch cột. Resize scrollbar xuất hiện/biến mất không reset vị trí history; chọn oldest trong 10k rows vẫn giữ 26 DOM rows.
- Sidebar/inspector splitters dùng delta clientX, cleanup pointer capture/cancel; resize main panel bằng kéo hai mép đã kiểm tra trong app demo.
- Final checks: 65 FE tests/15 files, check 0 errors/0 warnings, lint và production frontend build pass. Browser fixture/app @1280×800/1440×900 kiểm tra drag, keyboard, horizontal scroll, search và diff close/restore pass. Rust/IPC không đổi; không chạy lại Rust tests.
- `pnpm tauri build` pass: executable và ba gói Linux chứa bản sửa cuối đã build lại. Native UI trên repo thật **blocked/not run** vì không có native window automation; xem [handoff](handoffs/UI-graph-resize-scroll-2026-09-23.md), [QA](evidence/UI-graph-resize-scroll-2026-09-23/qa.md) và [build log](evidence/UI-graph-resize-scroll-2026-09-23/native-build.log).

## Follow-up — Context menu (2026-09-23)

- Thêm context menu dùng chung cho repository tabs, commit rows, Working changes files và Commit details files. Action dùng đúng callback/exact token hiện có; busy/read-only disable mutation.
- Mouse right-click và Shift+F10/Context Menu key cùng mở menu; Arrow/Home/End/Enter/Escape, outside-click, viewport-edge clamp và source highlight đã triển khai.
- Commit menu được mở rộng đủ 17 action theo ảnh user. `Create branch here…` là action khả dụng, truyền exact selected OID vào typed `branch_create`; 16 action chưa có typed Git engine hiển thị `Planned`/disabled với lý do. Menu cao tự scroll, hard reset dùng danger color.
- Regression fix: tách callback TopBar `openBranches()` khỏi `openBranchesAt(oid)` sau khi browser QA bắt `MouseEvent` bị lưu nhầm thành OID; final QA xác nhận HEAD/commit source không còn lẫn nhau.
- Browser QA @1280×720/800: Open details, Open diff, mock Stage file, keyboard Select file, menu sát mép, TopBar branch-from-HEAD và create `qa/regression` tại commit `e5f6071` pass; console sạch.
- Final checks: 72 FE tests/17 files, check 0 errors/0 warnings, lint, production build pass. Final `pnpm tauri build` ngoài sandbox exit 0, tạo executable + `.deb`/`.rpm`/`.AppImage`; lần build sandbox trước đó lỗi tại linuxdeploy vì cần tải runtime. Rust/IPC không đổi. Native UI repo thật **blocked/not run**; xem [handoff](handoffs/UI-context-menu-2026-09-23.md) và [QA](evidence/UI-context-menu-2026-09-23/qa.md).

## Fix push `AUTH_REQUIRED` (2026-09-23)

- Root cause UI: background Git job tạo `AppError` an toàn nhưng `JobOutcome` chuyển thành string code; `OperationRecord` chỉ giữ `errorCode`, làm frontend chỉ hiện `push failed (AUTH_REQUIRED)` và mất recovery.
- Backend giữ structured `AppError` trong terminal operation record, vẫn giữ `errorCode` để tương thích log cũ. Auth recovery đổi thành `authenticate`; raw stderr/remote secrets không đi qua IPC. `GIT_TERMINAL_PROMPT=0` vẫn giữ để app không treo prompt.
- Frontend phân biệt HTTPS và SSH từ URL đã redact: hướng dẫn refresh token trong credential helper hoặc start/unlock SSH agent, kiểm tra quyền repo, rồi Retry. Retry là click thủ công, không auto-replay push.
- Máy build có `credential.helper=store` và `SSH_AUTH_SOCK` được set; không đọc credential/key. Điều này chỉ ra app có đường dùng helper/agent, nhưng token/key/quyền trên repo thật vẫn cần user xác thực.
- Checks hiện tại: 75 FE tests/18 files, 119 Rust tests, check/lint/clippy/fmt và production build pass. Browser fixture @1280×720: banner, Retry callback và console sạch. Final `pnpm tauri build` exit 0, tạo lại executable + `.deb`/`.rpm`/`.AppImage`; checksums nằm trong evidence. Native push với remote thật **blocked/not run** vì không có repo/credential đích của user trong fixture; xem [handoff](handoffs/UI-push-auth-recovery-2026-09-23.md).

## Bitbucket Cloud integration (2026-09-23)

- App Password đã bị Bitbucket vô hiệu từ 09/06/2026. Flow mới dùng scoped API token với Repository Read + Write cho Bitbucket Cloud HTTPS; SSH vẫn dùng SSH agent.
- Typed `bitbucket_connect` trust/version/host-gated, yêu cầu configured Git credential helper và gửi token qua `git credential approve` stdin. Token không đi vào argv, env, remote URL, operation log hoặc app persistence; request không implement `Debug`.
- Fetch/Pull/Push Bitbucket dùng username từ remote hoặc `x-bitbucket-api-token-auth` để lookup đúng helper. UI có **Bitbucket auth**, error action **Connect Bitbucket**, link tạo token, password input và **Save & retry push**.
- Automated hiện tại: 77 FE tests/19 files và 122 Rust tests pass; integration test dùng credential-store cô lập trong `/tmp`, không chạm credential thật. Browser fixture @1280×720 kiểm tra modal/focus/scopes/hidden token/submit; console sạch. Final `pnpm tauri build` exit 0, tạo lại executable + `.deb`/`.rpm`/`.AppImage`; checksums nằm trong evidence.
- Native push Bitbucket thật **blocked/not run** vì không có API token/remote user fixture; app không đọc credential đang có. Xem [handoff](handoffs/UI-bitbucket-integration-2026-09-23.md) và [QA](evidence/UI-bitbucket-integration-2026-09-23/qa.md).

## Follow-up — Sidebar branch search và context menu (2026-09-23)

- Một ô search chung lọc cả Local/Remote/Tags (case-insensitive, count "N of M", Escape xóa).
- Menu branch theo ảnh user: Checkout, Merge into current (mở MergeModal, gồm cả remote source), Rebase/Cherry-pick/Revert (tái dùng modal + token flow, cherry-pick/revert ghi rõ chỉ lấy tip), Create branch/tag, Move branch tới commit đang chọn (token `branch_move`), Reset soft/mixed/hard, Rename, Set upstream, Push (tới upstream đã cấu hình), Push to, Copy, Reveal in History, Delete. Không có AI Code Review (ngoài phạm vi sản phẩm: không AI runtime).
- Backend mới: `branch_move/rename/set_upstream/push` + token `branch_move`; merge sources gồm remote refs.
- Checks: check 0 errors, lint sạch, **92/92 FE tests** (23 files; `branch-menu.test.ts` mới), **138/138 Rust tests** (3 mới), clippy/fmt sạch, production build pass. `.deb`/`.rpm` rebuild xong; AppImage fail `Text file busy` vì binary đang được app cũ chạy — cần tắt app rồi bundle lại.
- Remote luôn expand (bỏ điều kiện activeSection); click branch giữ đúng section theo loại ref (local/remote/tag) thay vì ép về local.

## Follow-up — Auto-stash khi checkout/switch (2026-09-23)

- Worktree dirty (staged/unstaged, không conflict) thì Checkout/Switch mở dialog "Stash & switch" thay vì báo lỗi khô: checkbox include-untracked (mặc định bật vì untracked cũng tính dirty), message stash cố định nhận diện được.
- Luồng: `stash_save` → `branch_switch`/`history_checkout` bằng version mới; không tự pop (tránh conflict bất ngờ), user restore tay từ Stashes. Stash lỗi thì không switch; switch lỗi thì changes vẫn an toàn trong stash, modal nêu rõ.
- Áp dụng cho sidebar Checkout, nút Switch/Track trong dialog Branches và Checkout commit trong history. Conflicted worktree giữ nguyên lỗi backend.
- Checks: check 0 errors, lint sạch, **104/104 FE tests** (`auto-stash.test.ts` mới), production build pass. Không đổi Rust.
- Ghi chú: modules highlight/side-by-side diff (thuần + tests) đã xong nhưng chưa nối vào DiffPane — parked, nối tiếp khi user yêu cầu lại.

## Working changes discard và partial hunk actions (2026-09-23)

- Bỏ checkbox và header “Working tree”; thanh trạng thái giữ changed count/branch + Refresh. File row dùng direct Discard/Stage/Unstage; context menu có Discard file changes.
- Worktree text diff có Stage hunk và confirmed Discard hunk. Backend-issued `hunkId` bind exact raw hunk; backend rebuild patch rồi `git apply` qua stdin. Untracked/binary/submodule/truncated không giả partial support.
- Discard file/hunk có modal default Cancel. Tracked file restore worktree từ index; untracked clean đúng một literal file. Confirmation bind fingerprint/hunkId và fail closed khi content đổi.
- Automated: 77 FE tests/19 files, 124 Rust tests, check/lint/clippy/fmt và frontend build pass. Browser QA @1280×800 pass; console sạch. Native Git mutations được test trong repo `/tmp`; native window trên repo thật chưa chạy.
- Release executable + `.deb` + `.rpm` + `.AppImage` đã rebuild. Sandbox AppImage fail tại linuxdeploy; rerun bundler với network pass. Xem [handoff](handoffs/UI-working-changes-hunks-2026-09-23.md) và [QA](evidence/UI-working-changes-hunks-2026-09-23/qa.md).
