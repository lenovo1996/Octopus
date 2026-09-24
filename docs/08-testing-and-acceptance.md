# Testing and acceptance

**Nguyên tắc:** UI demo chứng minh presentation; test Rust chứng minh Git primitives; native end-to-end chứng minh hai phần đã nối. Release cần cả ba loại bằng chứng thích hợp.

## 1. Test pyramid

| Layer | Test gì | Công cụ dự kiến | Bằng chứng |
|---|---|---|---|
| Pure/unit | byte parsers, argv validation, graph layout, DTOs, redaction | cargo test; Vitest | Test names + results |
| Component | keyboard, loading/error, selection, disabled reasons, draft | Testing Library/Svelte | Assertions theo user behavior |
| Rust integration | repo thật disposable, exact index/tree/refs, failures | temp dirs + system Git | OID/tree/file bytes trước và sau |
| Browser UI/visual | 3 panel states, resize, latest-request behavior | Playwright với mock adapter | Screenshots + interactions, gắn nhãn mock |
| Native E2E | Tauri IPC + Rust + Git thật + native window | WDIO Tauri service hoặc tauri-driver route | Native runner report + fixture evidence |

Tài liệu Tauri hiện mô tả WDIO embedded provider cho Linux/Windows/macOS; direct tauri-driver chỉ hỗ trợ Linux/Windows. T01/T15 pin route thực tế, proof-of-concept trên Linux trước; mọi test plugin/server chỉ vào test build và không được có trong release. [Tauri WebDriver](https://v2.tauri.app/develop/tests/webdriver/)

Không viết test để khẳng định literal CSS value hoặc mirror implementation. Chọn assertions có khả năng phát hiện lỗi người dùng sẽ gặp: chọn sai file, mất draft, graph edge sai, stale write, double commit.

## 2. Fixture catalog

Mọi fixture tạo trong temp directory riêng, Git config/identity cô lập, fixed timestamps, hooks mặc định disabled bằng test config trừ fixture test hooks. Không đọc/sửa repo thật hoặc global config. Tests cleanup chỉ temp root do harness tạo; failure giữ artifacts khi flag yêu cầu.

| ID | Fixture | Lỗi phải bắt |
|---|---|---|
| FX-01 | Empty repo → first commit → detached HEAD | Assume HEAD luôn tồn tại/luôn là branch |
| FX-02 | Linear, diverge, merge, fan-out, tag annotated/lightweight | Sai parent/lane/order/ref peeling |
| FX-03 | ≥401 commits, merge edge qua page boundary | Cursor duplicate/skip, lane đứt |
| FX-04 | Staged+unstaged cùng file, rename/delete/type change | Trộn index và worktree |
| FX-05 | Space/tab/newline/Unicode/leading dash/pathspec magic; non-UTF-8 Unix | Command injection, parse/sửa sai filename |
| FX-06 | Binary, large diff, symlink, gitlink, no-newline, mode-only | Render sai/crash/ghi ra ngoài root |
| FX-07 | Hai worktree + external CLI update + existing index.lock | Race, stale version, unsafe lock removal |
| FX-08 | Bare remote + hai clones; ff/diverged/non-ff push | Pull policy và push protection |
| FX-09 | Stash apply/pop success/conflict/reorder | Mất stash hoặc apply hai lần |
| FX-10 | Content/add-add/modify-delete/rename/binary merge conflicts | Stage incomplete result, nhầm side, complete sai |
| FX-11 | Hook/filter sentinel, trust denied, signing fail, identity missing, mocked auth/network fail | Execution trước trust, false success, secret leakage, auto retry |
| FX-12 | Shallow history, partial clone refusal, SHA-256 nếu support, missing repo, corrupted settings | Accidental lazy-fetch, hardcode 40-char hash, missing-parent crash, persistence loss |

## 3. Acceptance scenarios

### AT-01 — Daily local workflow

Given repo có một commit và ba changed files; When user stage hai file, nhập subject/body, commit; Then HEAD có đúng tree của index, message đúng, file thứ ba vẫn unstaged, graph chọn commit mới và draft clear sau verified success. Chạy native.

### AT-02 — Stage/unstage chính xác

Given path đặc biệt và unborn/normal repo; When stage/unstage selection; Then index entries đúng selection, worktree bytes không đổi khi unstage, rename gồm old/new đúng. Chạy Rust integration.

### AT-03 — Inspect topology

Given FX-02/03; When scroll qua trang và chọn merge parent; Then OIDs/edges đúng fixture, không trùng/mất row, diff so với parent đã chọn. Chạy layout unit + browser + một native integration.

### AT-04 — Remote collaboration

Given bare remote + clones A/B; When A push, B fetch/pull ff, B commit/push; Then remote OID đúng. Khi hai clones diverge, Pull báo DIVERGED và worktree không đổi; rejected push không force. Chạy local network/file-remote harness.

### AT-05 — Conflict completion

Given merge bắt đầu từ clean state; When conflict text được resolve/stage rồi Complete; Then merge commit có đúng 2 parents và no unmerged entries. Given stash conflict, Then stash còn và không hiện Complete merge. Unsupported conflict có external-resolution guidance. Chạy native + integration.

### AT-06 — Errors và cancellation

Given hook fail, process timeout, network disconnect hoặc IPC mất terminal event; Then draft còn, job có trạng thái đúng, polling phục hồi terminal, no automatic duplicate write. Cancel không hứa rollback, push unknown có explicit inspection. Chạy fault injection + native smoke.

### AT-07 — External changes

Given UI snapshot N; When terminal sửa HEAD/index hoặc file selected; Then watcher/focus refresh invalidates; request N bị reject khi recheck khác; graph/status không trộn hai repos khi user switch. Chạy integration + race-controlled browser.

### AT-08 — Package và accessibility

Given baseline máy sạch cài .deb; Then mở launcher, open repo, keyboard stage/commit/search/refresh đều dùng được; missing Git có guidance; release build không có test server; contrast và focus pass. Chạy clean environment/native manual+automated.

## 4. Requirement traceability

| FR | Task | Tests/gate |
|---|---|---|
| 01–05 | T03, T06, T11 | FX-01/07/08/12, AT-08, open/init/clone smoke |
| 06–07, 11 | T04–T06, T08 | FX-02/03/06/12, AT-03, search latest-request test |
| 08–10, 12 | T07–T10 | FX-01/04/05/07/11, AT-01/02/07 |
| 13–15 | T11 | FX-08/11, AT-04/06 |
| 16–17 | T12–T13 | FX-09/10, AT-05/06 |
| 18–21 | T02, T07, T14–T16 | Visual rubric, settings tests, redaction, AT-08 |
| 22–25 | P2 | MVP checks inactive/planned/read-only states; không yêu cầu P2 implementation |

## 5. Performance budgets

Mục tiêu thiết kế, cần benchmark trước khi pass. Máy chuẩn: Linux x86_64, 4 CPU cores, 16 GB RAM, SSD; ghi model CPU, distro, WebKit/Git/app versions và dataset actual. Không so số của hai máy khác nhau như regression.

| Phép đo | Dataset / điều kiện | Target |
|---|---|---|
| Time to usable history | 10k commits, 5k tracked files; từ Open đến first 200 interactive rows | Warm p95 ≤2s, cold ≤5s |
| Status refresh | Cùng repo, ≤100 changes, không external hooks | Warm p95 ≤1s |
| Small diff | ≤200 KiB text file | Warm p95 ≤300ms |
| Selection/scroll UI | Loaded rows, no network | Input response ≤100ms; p95 frame ≤33ms |
| Memory | Idle app + repo trên sau load 1k rows | Whole app process tree RSS ≤350 MiB mục tiêu |

Đo warm ≥20 lần, cold ≥5 lần, định nghĩa cold là app restart và cache state ghi rõ (không tự drop OS caches với sudo). Dataset 100k commits/50k files là stress case: phải responsive/cancellable và bounded, chưa hứa latency giống normal. Oversized response trả truncation, không OOM. Nếu fail budget, báo số thực và xử lý hoặc xin scope decision; không đổi target ngầm.

## 6. UI rubric

Chụp 1440×900 và 1280×800 cho Working changes, Commit details, Merge conflict. So với ảnh mẫu về topology và wireframes về token/spacing.

| Tiêu chí | Pass |
|---|---|
| Bố cục | Toolbar trên; sidebar trái; graph chiếm center; inspector phải; status dưới |
| Graph | Rows và nodes alignment, ≥3 visible lane colors ở fixture, labels không che node |
| Density | Row 32px và text đọc được; không đẩy nội dung vào oversized cards |
| Interaction | Resize, keyboard, selection, scroll, loading/empty/error không vỡ |
| Accessibility | Text contrast ≥4.5:1, visible focus, labels, không chỉ dựa màu |

Không đòi pixel-match chữ/data mờ của ảnh gốc. Không dùng SSIM score như bằng chứng duy nhất.

## 7. Release gate

- T01–T16 `done` với link evidence; P0/P1 acceptance pass, P2 disabled đúng.
- Không bug mất dữ liệu, mutate sai target, lộ secret, graph parent sai hoặc app crash ở workflow chính.
- Native Linux E2E + build/install smoke pass; checks bị môi trường block không tính pass.
- Known limitations, benchmark, source/dependency versions, installer checksum và diagnostics redaction review được ghi.
- UI review đủ 3 states; source license decision trước public publishing; công bố/push/release chỉ trong phạm vi chủ project yêu cầu.
