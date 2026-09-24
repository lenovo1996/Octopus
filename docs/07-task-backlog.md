# Task backlog

Task definitions là phạm vi giao việc; trạng thái thật nằm ở [progress.md](progress.md). Không đánh dấu task done tại hai nơi. `BE` = Rust/backend, `FE` = Svelte/frontend, `QA` = kiểm chứng; đây là vai trò, không yêu cầu nhiều agent.

## M0 — Foundation

### T01 — Scaffold và contract foundation

- **Vai trò / effort / depends:** Full-stack; 1–1.5 ngày; không.
- **Đọc:** [architecture](03-architecture.md), [IPC](04-ipc-contracts.md), [build](10-build-release.md).
- **Đầu ra:** scaffold Tauri 2/Svelte 5/TS/Vite tại root, scripts chuẩn, lockfiles, toolchain pin, typed error/result, mock/real adapter boundary, CI skeleton. Tạo tối thiểu preflight command và native IPC smoke.
- **Phạm vi file:** package/config files, `src/app`, `src/lib/ipc`, `src-tauri`, test config, CI; giữ docs và ảnh gốc.
- **Nghiệm thu:** `pnpm check`, `pnpm test:unit`, Rust tests/fmt/clippy và native dev launch pass trên Linux hoặc ghi môi trường blocker chính xác; app_preflight trả Git status thật; browser demo không gọi native API vô điều kiện.
- **Kiểm chứng:** DTO round-trip + command smoke. Pin và ghi versions; không khởi tạo lại/xóa metadata Git có sẵn.

### T02 — Application shell theo mẫu

- **Vai trò / effort / depends:** FE; 1–1.5 ngày; T01.
- **Đọc:** [UI spec](02-ux-ui-spec.md), [design](design/README.md), ảnh gốc.
- **Đầu ra:** topbar, toolbar, sidebar, history viewport, inspector, status bar; tokens; splitter; 3 inspector states bằng mock fixture rõ nhãn Demo data.
- **Phạm vi file:** `src/app`, `components`, styles, UI fixtures.
- **Nghiệm thu:** 1440×900 và 1280×800 giữ ba cột, no clipping; keyboard focus/resize dùng được; Planned controls đúng PRD; không thả card dashboard thay graph.
- **Kiểm chứng:** component interactions + screenshots 3 states, review theo rubric UI. Không cần test snapshot từng CSS rule.

## M1 — Read-only repo

### T03 — Git runner, repo discovery, open/init/recent

- **Vai trò / effort / depends:** BE + wiring; 1–2 ngày; T01.
- **Đọc:** [Git engine](05-git-engine.md) mục 1–3, [security](09-security-and-data.md).
- **Đầu ra:** bounded subprocess runner, repo registry, repo_open/init/close/recent, Git preflight, trust state, atomic recent persistence; Open/Init UI nối thật. Init cần trust/explicit user intent nhưng không sửa existing repo.
- **Nghiệm thu:** ordinary/linked worktree/subfolder discover đúng; unborn không lỗi; bare/missing Git/permission có error code; không chạy shell/config helpers ngoài ý muốn; close busy bị chặn.
- **Kiểm chứng:** isolated fixture tests và native dialog/open smoke. `.git` có thể là file, không dùng path string từ UI để thay repo context sau open.

### T04 — History/ref reader và pagination

- **Vai trò / effort / depends:** BE; 1–1.5 ngày; T03.
- **Đầu ra:** repo_refs, history_page, commit_details, fixed-tip sessions, paged metadata; tests cho root/merge/shallow/detached/tag.
- **Nghiệm thu:** OID và parents đúng Git; page size ≤200; không duplicate/skip trên stable session; refs change invalidation có state rõ; invalid cursor/commit bị reject.
- **Kiểm chứng:** đối chiếu raw Git fixtures, message chứa delimiter/newline, SHA-256 repo nếu Git support; log payload limits.

### T05 — Commit graph và history integration

- **Vai trò / effort / depends:** FE; 1–1.5 ngày; T02, T04.
- **Đầu ra:** pure graph lane layout, virtual list, refs badges, selection, pagination, commit details basic nối thật.
- **Nghiệm thu:** merge edge đúng, lane tiếp nối qua page, dot/row alignment ổn khi scroll/resize; 10k commits không render 10k DOM nodes; screen reader đọc semantic list.
- **Kiểm chứng:** topology invariants, page-boundary/branch fan-out fixtures, screenshot với same fixture; count rendered rows.

### T06 — Navigation, branch scope và search

- **Vai trò / effort / depends:** Full-stack; 0.5–1 ngày; T05.
- **Đầu ra:** sidebar refs/tags/stashes read-only, scope selector, history_search có cancellation, Show in graph, loading/empty/error.
- **Nghiệm thu:** selected ref hiển thị reachable history; search backend vượt trang đã load; latest query wins; detached/unborn labels đúng; P2 sections có explanation.
- **Kiểm chứng:** out-of-order responses, đổi repo trong lúc search, không lẫn result và graph topology.

## M2 — Local Alpha

### T07 — Worktree status, refresh và operation coordinator

- **Vai trò / effort / depends:** Full-stack; 1–1.5 ngày; T03.
- **Đầu ra:** porcelain v2 byte parser, ChangedFile tokens, snapshot versions, watcher/focus refresh, per-common-dir queue, job registry và IPC events.
- **Nghiệm thu:** staged+unstaged cùng file đúng; rename/delete/untracked/conflict đúng; linked worktree shared lock; external change invalidates stale token; double-submit dedup; listener race có operation_get fallback.
- **Kiểm chứng:** adversarial path fixture, stale version, watcher storm, index.lock failure; không tự xóa lock.

### T08 — Diff viewer

- **Vai trò / effort / depends:** Full-stack; 1–1.5 ngày; T04, T07.
- **Đầu ra:** diff_read, unified viewer, expand/back, parent comparison, binary/large/symlink/gitlink fallback.
- **Nghiệm thu:** đúng source worktree/index/commit; rename path identity không parse từ display; untracked preview không sửa index; root và second-parent diff đúng; no-final-newline rõ.
- **Kiểm chứng:** text/binary/oversize/rename/non-UTF-8/mode-only cases, exact output bounds; HTML/script trong file chỉ render text.

### T09 — Stage và unstage

- **Vai trò / effort / depends:** Full-stack; 0.5–1 ngày; T08.
- **Đầu ra:** per-file/all stage/unstage, partial hunk stage, confirmed file/hunk discard với WriteContext, exact path IDs và backend-issued hunk IDs. UI không dùng checkbox selection.
- **Nghiệm thu:** stage một file/hunk chỉ đổi đúng index target; worktree bytes nguyên khi unstage; discard tracked chỉ restore unstaged side; discard untracked chỉ xóa exact file; stale confirmation/hunk fail closed; unborn/rename/deletion/pathspec magic đúng.
- **Kiểm chứng:** compare `ls-files --stage`, cached/worktree diff và file bytes trước/sau trong repo tạm; concurrent edit trả stale/refreshed state đúng.

### T10 — Commit và branch workflow

- **Vai trò / effort / depends:** Full-stack; 1–2 ngày; T06, T09.
- **Đầu ra:** commit editor/draft, create/switch/delete branch dialogs, identity read-only, local alpha demo.
- **Nghiệm thu:** commit chỉ index; failed hook/signing giữ draft và giải thích; empty/unresolved chặn; branch delete safe + confirmation; current/other-worktree branch được bảo vệ; create+switch partial outcome rõ.
- **Kiểm chứng:** Daily commit E2E, identity missing, hook fail, detached commit, unborn first commit, branch unmerged refusal; checks toàn M2.

## M3 — Linux MVP features

### T11 — Clone và remote sync

- **Vai trò / effort / depends:** Full-stack; 1.5–2 ngày; T10.
- **Đầu ra:** clone UI/job, remote_list/fetch/pull/push, protocol validation, auth recovery, progress/cancellation, destination handling.
- **Nghiệm thu:** bare remote + hai copies sync đúng; dirty/diverged pull không thay đổi files; first push explicit upstream; non-fast-forward không force; URL secrets không log; partial clone không bị auto delete.
- **Kiểm chứng:** local remote suite + opt-in SSH/HTTPS smoke khi có môi trường; simulate network drop và unknown push outcome; không dùng credentials thật trong fixtures.

### T12 — Stash workflows

- **Vai trò / effort / depends:** Full-stack; 0.5–1 ngày; T10.
- **Đầu ra:** stash list/save/apply/pop, include-untracked checkbox, OID identity, conflict status.
- **Nghiệm thu:** tracked-only default; untracked chỉ vào stash khi chọn; pop conflict giữ stash; concurrent stash reorder không tác động nhầm; apply-success/drop-failure không apply lại.
- **Kiểm chứng:** tracked/untracked/binary fixture, pop conflict và retained OID, staged state không được hứa restore do không dùng --index.

### T13 — Merge và conflict inspector

- **Vai trò / effort / depends:** Full-stack; 1.5–2 ngày; T11, T12.
- **Đầu ra:** merge preview/start, conflict stages, supported whole-file resolutions, external editor workflow, mark resolved, complete/abort.
- **Nghiệm thu:** clean precondition; --no-ff --no-commit flow; incomplete resolution không complete; resolved merge có 2 parents; abort confirmation không fallback destructive; stash/external merge khác app-origin merge.
- **Kiểm chứng:** content, add/add, modify/delete, rename, binary, symlink, submodule; unsupported cases có external guidance và không ghi sai file. Complete/abort unavailable khi không hợp lệ.

### T14 — Preferences, operation feedback và polish

- **Vai trò / effort / depends:** Full-stack; 1–2 ngày; T13.
- **Đầu ra:** settings v1/migrations, persistence panel widths/font/drafts, identity menu, help/diagnostics, error recovery, accessible shortcuts.
- **Nghiệm thu:** restart giữ prefs/draft đúng repo; corrupted settings backup và default; errors có action rõ; keyboard và contrast pass; no production mock fallback; release không có test-only IPC.
- **Kiểm chứng:** settings migration, repo switch/draft isolation, shortcut input scope, redaction corpus, 3 inspector visual review.

## M4 — Release candidate

### T15 — Integration, edge cases và performance gate

- **Vai trò / effort / depends:** QA + fixes; 1.5–2.5 ngày; T14.
- **Đầu ra:** toàn [QA matrix](08-testing-and-acceptance.md), native automation, performance report, visual evidence, bug fixes trong scope.
- **Nghiệm thu:** P0/P1 mapped đến passing evidence; không unresolved data-loss/topology bug; benchmark đúng hardware/dataset ghi lại; cancellation/race/restart scenarios pass.
- **Kiểm chứng:** browser mock, Rust fixtures và native . Không gộp chúng thành “E2E pass” nếu chỉ chạy browser.

### T16 — Linux package và handoff

- **Vai trò / effort / depends:** Release; 1.5–2.5 ngày; T15.
- **Đầu ra:** reproducible .deb build, CI artifact/checksum, installation/runbook, known limitations, release notes draft; public publishing cần chủ project quyết định.
- **Nghiệm thu:** install/launch/uninstall trên baseline sạch, data không tự bị xóa khi uninstall; Git missing flow đúng; production không có embedded test server; source/dependency license inventory đủ, app license cần chủ project chốt trước public release.
- **Kiểm chứng:** [release checklist](10-build-release.md); package chạy workflow local và local-remote; artifact có version/commit/checksum. Chưa có Git commit thì ghi rõ source snapshot identifier, không bịa commit hash.

## Mở rộng theo yêu cầu user

### T19 — Lưu và khôi phục workspaces đã mở (yêu cầu trực tiếp 2026-09-23)

- **Vai trò / depends:** Full-stack; T03 (open/recents/trust/store) và T17 (tabs) đã có implementation. Yêu cầu user cho phép làm trước gate M4; các mục P2 khác giữ nguyên.
- **Đầu ra:** `workspaces_save`/`workspaces_restore` typed IPC, `openWorkspaces` + `activeWorkspace` trong store v1, tự lưu khi tabs/active đổi (debounce), tự mở lại khi launch, skip + prune repo đã mất, giữ trust từ cache.
- **Nghiệm thu:** restart mở lại đúng tabs theo thứ tự + tab active; repo bị di chuyển/xóa được bỏ qua có thông báo và không thử lại lần sau; key giả không mở được gì; đóng hết tabs lưu rỗng và launch sau về Welcome; draft theo worktree vẫn còn.
- **Kiểm chứng:** core save/restore tests trên repo tạm, FE unit cho payload/active resolve, gates FE/BE sạch. Native restart workflow ghi riêng.

### T17 — Nhiều repository tabs (yêu cầu trực tiếp 2026-09-23)

- **Vai trò / depends:** Full-stack; T03/T05/T08/T14 đã có implementation. Yêu cầu user cho phép đưa multi-repo tabs lên trước gate M4; các mục P2 khác giữ nguyên.
- **Đầu ra:** thanh tab repo/branch/dirty/draft/busy, picker Recent + native folder multi-selection, workspace độc lập, close từng tab, phím tắt, stable draft identity theo worktree.
- **Nghiệm thu:** không trộn status/draft/diff/operation giữa repo; mở cùng canonical worktree chọn tab đã có; linked worktrees giữ HEAD/index riêng nhưng chung write queue; close busy bị từ chối; close lỗi giữ tab và draft; close tab cuối về Welcome; overflow không tràn app.
- **Kiểm chứng:** native registry integration trên repo tạm, mock isolation tests, browser tab lifecycle/keyboard/visual 1280×800 và 1440×900. Không tự restore tabs sau app restart; draft vẫn persist và được đọc khi user mở lại repo. Native UI gate phải ghi riêng.

## P2 — Chỉ mở sau M4, trừ ngoại lệ T17 theo yêu cầu user

| Nhóm | Backlog cần RFC/acceptance mới |
|---|---|
| Advanced Git | Limited undo/redo, recovery refs, amend/revert/rebase/cherry-pick, hunk staging |
| Providers | GitHub/GitLab PR read/review/actions, credential/profile management |
| Repository types | Submodule mutation, worktree manager, Git LFS-specific workflows |
| Platforms | Windows installer/paths/SSH; macOS signing/notarization/keychain; native tests từng OS |
| Polish | Light theme, localization tiếng Việt, opt-in signed updater; multi-repo tabs chuyển sang T17 |
