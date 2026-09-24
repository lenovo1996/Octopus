# T04 — Handoff (backend; FE DTOs sang T05)

- Date: 2026-09-22
- Status: done (phạm vi backend theo backlog; nối UI ở T05/T06)
- Milestone/gate: M1 tiếp tục
- Requirements covered: FR-06/FR-07 (phần backend), acceptance T04 (OID/parents,
  page ≤200, no dup/skip, invalidation, invalid cursor/commit)

## Điều hiện đã hoạt động

- `repo_refs`: `for-each-ref` với `%00` separators (đã verify bytes), annotated tag
  peel về commit, `current` từ HEAD, `checkedOutElsewhere` từ `worktree list`.
- `history_page`: tips pin theo scope (allRefs/head/ref) → `rev-list --topo-order
  --parents --boundary` → session cache (tối đa 32, cursor opaque `{id}:{offset}`).
  Metadata qua `cat-file --batch` parse length-framed (không separator trong message).
- `commit_details`: validate oid hex theo object-format session, parse commit object
  (subject/body/author/ISO dates), file list từ `diff-tree --name-status -z -r`
  (rename OLD→NEW đã verify thứ tự bytes); merge bắt buộc chọn parent, root chỉ
  chấp nhận `parentIndex: null`.
- `history_search`: literal case-insensitive trên subject/author/oid-prefix (không
  regex), paginate, `incomplete: false`.
- ISO dates tự viết (Hinnant), không thêm dep chrono.

## Bugs thật tìm được qua tests (đã fix + regression test)

1. `--git-common-dir` tương đối theo cwd — đã fix ở T03 (ghi lại để nhớ).
2. Body parse giữ dòng trống phân cách (`subject\n\n\nbody`) — fix: trim theo
   convention git, oracle `%B`.
3. `civil_from_days` sai 3 chỗ: không gán `doe` về `days`, chia đôi timestamp thay
   vì cộng offset, `day_secs` từ day-count — fix cả ba, thêm assert date 2023.
   Nếu không có test oracle, mọi timestamp 2026 đã hiện 1970.
4. Rename destructuring đảo OLD/NEW — fix theo thứ tự bytes `R100\0OLD\0NEW\0` đo thật.
5. `rev-list --boundary` KHÔNG đánh dấu grafted commit reachable bằng `-` —
   boundary đọc từ `$GIT_DIR/shallow` (+ common-dir). Phát hiện bằng `od -c`.

## Checks và evidence

| Command / procedure | Result | Evidence | Giới hạn |
|---|---|---|---|
| `cargo test --locked` | passed | 28/28 (15 cũ + 9 fixture T04 + 4 parser unit): refs peel, pagination 65 commits đối chiếu `rev-list` oracle, merge parents, tricky message vs `%B`, root/OID/limit/cursor rules, search literal + page, detached/unborn, ref-scope, shallow boundary | Fixtures trên temp dirs, identity `-c`, dates cố định |
| `cargo clippy --all-targets --locked -- -D warnings` | passed | exit 0 | — |
| `cargo fmt --check` | passed | exit 0 | — |
| Native / FE | chưa chạy | Giữ lời không reload app user đang dùng; IPC mới chưa có TS DTOs | T05 thêm DTOs + nối HistoryPane rồi verify native một lượt |

## Quyết định và thay đổi contract

- Cursor search offset-only (`search:{offset}`, re-scan mỗi page) thay vì session:
  đơn giản, UI latest-wins; ghi rõ trong code.
- `commit_details` file list trả display paths (lossy) — tokens exact path là T08.
- Không đổi contract; TS DTOs cho 4 commands mới sẽ thêm ở T05 cùng parity test.

## Blockers / known issues

- Không blocker backend. User idle từ 12:17, app vẫn chạy nguyên.

## Next action

**T05 — Commit graph và history integration** (depends T02, T04-done): thêm TS DTOs,
pure lane layout, virtual list nối `history_page`/`commit_details` thật, rồi verify
native khi user rảnh.
