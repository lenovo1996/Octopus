# Product requirements

`P0` = Local Alpha; `P1` = bắt buộc cho Linux MVP; `P2` = sau MVP. MVP không được gắn nhãn hoàn thành khi mới xong P0.

## A. Repository và điều hướng

| ID | Mức | Yêu cầu và acceptance criteria | Task |
|---|---|---|---|
| FR-01 | P0 | Open folder qua native dialog; chấp nhận worktree hoặc thư mục con; resolve đúng root; báo not-a-repo/bare/missing Git rõ ràng | T03 |
| FR-02 | P0 | Init chỉ trong thư mục người dùng chọn và xác nhận; không ghi đè repo có sẵn; empty repo hiển thị unborn HEAD | T03 |
| FR-03 | P1 | Clone HTTPS/SSH hoặc local path, chọn destination mới/rỗng; progress/cancel; giữ partial clone và hướng dẫn cleanup sau lỗi | T11 |
| FR-04 | P0 | Recent repos tối đa 20, chọn lại đúng repo; repo bị di chuyển có Remove/Open another; lưu local | T03 |
| FR-05 | P0 | Sidebar local branches, remote refs, tags, stashes; đúng HEAD/current branch, detached/unborn state | T06 |

## B. Lịch sử và thay đổi

| ID | Mức | Yêu cầu và acceptance criteria | Task |
|---|---|---|---|
| FR-06 | P0 | Graph theo parent OID, hỗ trợ merge/root, pagination 200 rows, branch labels; không nối nhầm khi tải trang tiếp | T04, T05 |
| FR-07 | P0 | Chọn commit mở metadata, parents, file list, text diff; merge commit chọn parent để compare; binary/oversize có fallback | T04, T08 |
| FR-08 | P0 | Worktree chia unstaged/staged/conflicted; một file có thể ở cả staged và unstaged; refresh sau thay đổi ngoài app | T07 |
| FR-09 | P0 | Stage/unstage từng file hoặc selection/all; xử lý rename/delete/untracked và unborn HEAD; không stage file khác selection | T09 |
| FR-10 | P0 | Commit subject bắt buộc, body tùy chọn; chỉ commit index; chặn empty index, thiếu identity, unresolved conflict; giữ draft khi lỗi | T10 |
| FR-11 | P0 | Search message/author/OID trên scope lịch sử đã chọn ở backend; cancel request cũ; “Search results” không giả graph liên tục cho tập kết quả rời rạc | T06 |

## C. Nhánh và đồng bộ

| ID | Mức | Yêu cầu và acceptance criteria | Task |
|---|---|---|---|
| FR-12 | P0 | Create branch từ OID đã xác thực, switch branch, delete branch đã merge với confirmation; không force delete; hỗ trợ detached bằng Create branch | T10 |
| FR-13 | P1 | Fetch remote do người dùng chọn; refresh refs; ahead/behind ghi rõ dựa trên lần fetch cuối, không giả realtime | T11 |
| FR-14 | P1 | Pull theo fast-forward-only, không auto stash; chặn dirty worktree theo policy MVP và divergence; gợi ý merge thủ công | T11 |
| FR-15 | P1 | Push normal đến remote/branch đã chọn, hiện rõ đích; lần đầu xác nhận set upstream; reject non-fast-forward và không tự force | T11 |
| FR-16 | P1 | Stash tracked, tùy chọn include untracked, message; apply/pop; pop conflict giữ stash; không drop/clear trong MVP | T12 |
| FR-17 | P1 | Explicit merge với clean index/worktree; preview source/target, merge không tự commit; resolve bằng current/incoming/external edit; mark resolved và complete/abort có kiểm tra | T13 |

## D. Vận hành và UI

| ID | Mức | Yêu cầu và acceptance criteria | Task |
|---|---|---|---|
| FR-18 | P0 | Toolbar, sidebar, graph, inspector, status bar cùng cửa sổ; splitter resize và keyboard access; ảnh tham chiếu là chuẩn bố cục | T02, T05 |
| FR-19 | P1 | Preferences local: panel widths, font size 12–16px, recent repos; menu hiển thị effective Git identity và scope nguồn, không tự đổi Git config | T14 |
| FR-20 | P1 | Mỗi operation có trạng thái, lỗi có hành động phục hồi; retry mutation không tự động; busy/locking không tạo double commit/push | T07, T14 |
| FR-21 | P1 | Linux installable .deb, mở bằng desktop launcher, Git preflight, report diagnostics được redact; trợ giúp local, không tự gửi feedback | T16 |

## E. Phạm vi giữ chỗ từ ảnh mẫu

| ID | Mức | Phạm vi sau MVP | Cách hiện ở MVP |
|---|---|---|---|
| FR-22 | P2 | Undo/redo với operation journal và recovery refs | Ẩn toolbar action, không dùng undo UI cho Git history |
| FR-23 | P2 | Pull Requests qua provider adapter/auth | Sidebar mục tắt, nhãn “Planned”; mở explanation, không giả có PR |
| FR-24 | P2 | Submodule init/update/sync và conflict chuyên biệt | Read-only summary/gitlink; control mutation bị vô hiệu hóa có lý do |
| FR-25 | P2 | Quản lý credential/profile nhiều tài khoản, rebase/cherry-pick, hunk staging, tag mutation | Không có active controls; tên Profile trong ảnh ánh xạ thành Identity info ở MVP |

## Workflow nghiệm thu chính

1. **Daily commit:** Open → chọn Working changes → đọc diff → stage hai trong ba file → nhập message → Commit → thấy node mới, file thứ ba còn unstaged.
2. **Inspect history:** click ref → graph đúng phạm vi reachable → click merge commit → chọn parent thứ hai → diff đúng với parent đó.
3. **Collaborate:** fetch → thấy behind → Pull fast-forward → sửa/commit → Push với upstream đúng → bare remote chứa OID mới.
4. **Conflict:** clean repo → merge branch → inspector Conflict → resolve file → stage resolution → Complete merge → graph có đúng hai parents.
5. **Recover:** một thao tác thất bại hoặc repo bị chỉnh bên ngoài → lỗi có nguyên nhân → refresh → UI khớp Git, draft còn và không lặp mutation.

## Phi chức năng

- Không shell interpolation, HTML injection từ repo, credential persistence hoặc telemetry tự động.
- Keyboard cho open, search, refresh, selection, stage và commit; contrast text tối thiểu 4.5:1, focus thấy rõ, màu không là tín hiệu duy nhất.
- Mutation serialize theo common Git directory; reject snapshot cũ; refresh khi focus/window watcher event. Không hứa atomicity với Git chạy ngoài app.
- Giới hạn dữ liệu, latency/memory và matrix edge cases theo [QA](08-testing-and-acceptance.md).
- Mỗi control ở MVP làm việc thật, hoặc có disabled reason; mock chỉ dùng demo/test mode tách biệt.
