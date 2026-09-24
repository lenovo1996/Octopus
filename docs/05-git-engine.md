# Git engine behavior

## 1. Process runner

Rust spawn system Git bằng executable path đã xác thực và argv từng phần. Không `sh -c`, `bash -c`, string interpolation hay frontend-supplied subcommand. `cwd` là canonical worktree root từ registry. Reject environment Git override từ frontend; sanitize các biến inherited có thể đổi repository như `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`.

Read commands dùng config override như `color.ui=false`, `core.pager=cat`, `core.fsmonitor=false`; diff thêm `--no-ext-diff --no-textconv`. `GIT_TERMINAL_PROMPT=0`, pager/editor noninteractive theo loại thao tác; auth/signing cần hướng dẫn nếu tương tác bị chặn. Trong read-only trust mode, không chạy network, hooks, filters, arbitrary remote helpers. Không âm thầm sửa Git config của repo/user.

Read-only trước trust chỉ đọc refs/objects/history bằng allowlist; **chưa chạy status/worktree diff**, vì Git có thể cần content conversion khi so worktree với index. Khi trusted, status dùng `GIT_OPTIONAL_LOCKS=0` để tránh optional index refresh writes; không dùng biến này cho mutations. Read-only partial/promisor clones không hỗ trợ ở MVP: phát hiện và từ chối trước object access để tránh lazy fetch; chỉ hỗ trợ lại khi có cơ chế no-lazy-fetch được capability-test đúng Git baseline. Shallow clone đã đủ objects vẫn là case hỗ trợ.

Content filters là chương trình cấu hình qua Git attributes/config, vì vậy việc “chỉ đọc” phải được xác định theo command và conversion path, không chỉ theo tên thao tác UI. [Git attributes](https://git-scm.com/docs/gitattributes)

Timeout mặc định đề xuất: reads 30s, local writes 120s, network 300s có progress/extension chủ động. Đọc stdout/stderr đồng thời tránh pipe deadlock; giới hạn buffers và stream parse, kill/reap khi vượt ngưỡng. Log đã redact. Không biến exit code khác 0 thành cùng một loại lỗi: diff `--exit-code`/`--no-index` có semantics riêng.

Tên/path có `-`, spaces, tabs, newline, Unicode hoặc pathspec magic phải là literal. `--` tách options khỏi paths nhưng **không tự tắt pathspec magic**; dùng `--literal-pathspecs` hoặc strategy literal tương đương và NUL pathspec input khi subcommand hỗ trợ. Validate revision/ref trước khi tạo argv. [Git global options](https://git-scm.com/docs/git)

## 2. Snapshot, validation và race

Repo registry lưu worktree root, git-dir, common-dir, object format, trust, observed state và version. Repo discovery phải hỗ trợ `.git` là file ở linked worktree; không giả `.git/` luôn là thư mục.

Một mutation:

1. Lấy write queue theo common-dir, kiểm tra requestId và expectedVersion.
2. Rescan HEAD/ref/index và fingerprint target files; nếu khác observed snapshot thì tăng version và trả `STALE_STATE` trước khi write.
3. Kiểm trust, operation state, file/ref token, confirmation và preconditions của action.
4. Chạy Git, thu exit/error, xác minh postconditions bằng đọc state; không chỉ dựa vào stdout.
5. Rescan, tăng version nếu state đổi, invalidate caches, emit terminal record.

Giữa recheck và Git vẫn có race từ process ngoài app. Dựa vào lock/CAS nơi Git hỗ trợ, chấp nhận lỗi có kiểm soát và refresh; không tự retry mutation hoặc quảng cáo “atomic toàn bộ”. Dùng native Git locks, không tự tạo/xóa `.git/index.lock`. Detect active rebase/cherry-pick/bisect ngoài app: đọc được nhưng mutation liên quan bị chặn với `externalOperation`.

## 3. Read recipes

| Nghiệp vụ | Git primitive / quy tắc |
|---|---|
| Discover | `rev-parse` lấy toplevel, absolute git-dir/common-dir, bare, object format; empty HEAD là state hợp lệ |
| Status | `status --porcelain=v2 -z --branch --untracked-files=all`; parse bytes, record types `1`, `2`, `u`, `?`, `!`, headers unknown bỏ qua |
| Refs | `for-each-ref` với format machine-readable và field separators; peel annotated tag để history dùng commit đúng |
| History | `rev-list --topo-order --parents` trên tips đã resolve/pin; HEAD detached cần seed riêng; paged metadata từ batch object reads |
| Commit contents | `cat-file --batch` hoặc equivalent length-framed reader; không parse message bằng separator có thể xuất hiện trong message |
| Diff metadata | Raw/name-status với `-z`, tách path identity khỏi patch text; rename old/new là hai raw paths |
| Worktree diff | `diff --no-ext-diff --no-textconv --` + literal tracked paths; untracked dùng safe file preview/diff riêng, không add tạm vào index |
| Index diff | `diff --cached --no-ext-diff --no-textconv`; unborn hỗ trợ diff index với empty tree đúng object format |
| Commit diff | Compare exact parent OID và commit OID; root dùng empty tree; merge mặc định first parent, cho chọn parent |

Porcelain v2 cung cấp trạng thái index/worktree riêng và NUL path records; không split toàn output theo newline hoặc spaces. [Git status](https://git-scm.com/docs/git-status)

Topo order phải giữ children trước parents; timestamps không thay thế topology. Branch decorations không phải parent edges. [Git rev-list](https://git-scm.com/docs/git-rev-list)

Diff có trường hợp binary, rename, symlink, mode-only, no-final-newline; `--no-ext-diff` và `--no-textconv` tránh chạy external diff/text conversion. [Git diff](https://git-scm.com/docs/git-diff)

## 4. Local writes

| Nghiệp vụ | Precondition và hành vi |
|---|---|
| Init | Destination do user chọn; không có existing Git; branch name hợp lệ; không xóa nội dung sẵn có |
| Stage files | Exact listing tokens; `git add` với literal paths; stage rename gồm old + new khi cần để ghi nhận deletion; không dùng `add .` thay selection |
| Unstage files | Có HEAD: restore index từ HEAD cho exact paths, không đụng worktree. Unborn: remove exact entries khỏi index bằng cached-only primitive; không xóa file trên disk |
| Commit | Index không rỗng, identity đủ, không unresolved; message qua stdin (`-F -`) không argv/editor; giữ hooks/signing trong trusted mode; fail giữ draft |
| Create branch | Validate branch name bằng Git + UI rule không option-like; start là exact commit OID; switchAfterCreate là hai bước với partial result rõ |
| Switch | MVP yêu cầu clean worktree/index, không force; nhánh checked out ở worktree khác không switch; remote selection có flow tạo local tracking explicit |
| Delete branch | Không current/protected target, không checked out elsewhere; safe `-d`, confirmation token; refusal không fallback `-D` |

Identity thiếu: show config source và hướng dẫn người dùng cấu hình; không tự sửa global name/email. Detached HEAD commit được phép sau cảnh báo UI và khuyến nghị Create branch; không tự gắn commit lên branch khác. Message subject tối đa 500 ký tự, body tối đa 64 KiB; 72 ký tự là soft guideline, không hard reject.

## 5. Network và authentication

Remote URL nhận HTTPS, SSH, SCP-like SSH và local folder đã chọn. Reject `ext::`, unknown schemes, URL có inline password/token và custom transport helpers ở MVP. User biết clone/fetch/push liên hệ remote nào. Local test dùng bare remote, không cần tài khoản thật.

Auth dùng credential helper và SSH agent hiện có trong **trusted** operation; app không giữ token hoặc thay SSH config. Không bỏ host key checks. Prompt/passphrase không đáp ứng được → terminal operation giữ structured `AUTH_REQUIRED`/`authenticate`; UI phân biệt HTTPS/SSH từ URL đã redact, hướng dẫn refresh token hoặc start/unlock agent rồi cho Retry thủ công. Không auto-retry push. Redact URL userinfo, query secrets, raw stderr và credential-bearing environment.

Bitbucket Cloud HTTPS có flow explicit `bitbucket_connect`: user paste API token scoped, backend xác nhận remote host chính xác `bitbucket.org`, credential helper đã cấu hình và gửi credential protocol qua stdin tới `git credential approve`. Token không vào argv/env/URL/log/app settings. Username lấy từ HTTPS userinfo nếu có, nếu không dùng `x-bitbucket-api-token-auth`; Fetch/Pull/Push truyền `credential.username` bằng fixed Git `-c` argument để lookup cùng credential. App Password không được dùng vì Bitbucket đã vô hiệu chúng từ 09/06/2026. API token để push cần cả `read:repository:bitbucket` và `write:repository:bitbucket`. [Atlassian: Using API tokens](https://support.atlassian.com/bitbucket-cloud/docs/using-api-tokens/)

- Fetch explicit remote; không auto-fetch lúc mới mở repo. Không prune tự động trong MVP.
- Pull explicit current branch/upstream; backend enforce ff-only bất kể repo config; clean-worktree precondition, không autostash hoặc tự rebase. Divergence cần explicit merge flow.
- Push default normal; show remote + destination + source OID; set upstream chỉ sau user chọn. Không `--force`, không `--force-with-lease` trong MVP, không `--mirror`/`--all` implicit.
- Clone mutex theo canonical destination parent/name; race/permission làm fail an toàn. Nếu partial files, giữ và báo đường dẫn; UI không xóa tự động khi cancel.

## 6. Stash

Mặc định chỉ tracked; include untracked phải explicit. Không include ignored ở MVP. Stash item định danh bằng OID, index `stash@{n}` chỉ resolve lại ngay trước action và xác thực OID. Apply mặc định không `--index`; staged state gốc không được hứa phục hồi.

Pop dùng apply rồi drop đúng entry chỉ sau apply thành công và xác minh identity; conflict thì giữ stash. Nếu drop thất bại sau apply, báo partial success và không apply lại. `stash drop/clear` riêng chưa đưa vào UI. Stash conflict chuyển Conflict panel nhưng không có `merge_complete`/`merge_abort`; chỉ resolve và commit bình thường khi index sạch conflict. [Git stash](https://git-scm.com/docs/git-stash)

## 7. Merge và conflict

Chỉ bắt đầu merge từ branch hiện tại có HEAD, clean worktree/index, không active operation; nguồn là exact validated ref/OID. MVP explicit merge dùng `--no-ff --no-commit` để có bước review trước khi tạo merge commit, khác với pull fast-forward. Nếu already-up-to-date, no-op success. Tắt autostash; không merge unrelated histories tự động.

Giữ pre-merge HEAD và app-origin marker ngoài repo. Conflicts đọc unmerged index stages: base=1, current=2, incoming=3. Thiếu stage là trường hợp hợp lệ (add/add, modify/delete), không tạo nội dung rỗng giả. UI dùng tên branch/OID để giải thích “current/incoming”.

Resolve current/incoming chỉ cho regular-file text conflict được hỗ trợ; lấy stage bytes, validate target path và working fingerprint, atomic replace regular file. Symlink/submodule/binary/rename conflict chuyển external resolution hoặc action riêng được test; không follow symlink để ghi ngoài root. Deletion resolution stage deletion rõ ràng. Chỉ Mark resolved mới ghi index, không đồng thời auto commit.

Complete merge chỉ khi `MERGE_HEAD` tồn tại, head đúng dự kiến, không unmerged entries và user xem staged result. Abort chỉ cung cấp cho merge do app khởi tạo từ clean state, preview hậu quả và confirmation; Git abort có thể không khôi phục thay đổi tạo sau khi merge bắt đầu. Không tự fallback reset/clean nếu abort fail. Merge phát hiện từ terminal: inspect/resolve, hướng dẫn terminal completion/abort ở MVP để không hứa recovery không có origin snapshot.

`--no-commit` một mình không chặn fast-forward; ghép `--no-ff` để bảo đảm bước dừng. Hành vi abort và dirty state phải nghiệm thu. [Git merge](https://git-scm.com/docs/git-merge)

## 8. Partial staging và discard theo yêu cầu mở rộng

`diff_read` cấp `hunkId` từ exact raw hunk bytes. `diff_hunk_stage` rebuild patch hiện tại, resolve `pathId`, tìm đúng `hunkId`, rồi feed patch backend-built qua stdin vào `git apply --cached`; frontend không gửi patch text. `diff_hunk_discard` dùng cùng cơ chế với `git apply --reverse` và bắt buộc confirmation token. Untracked/binary/submodule/truncated diff không có partial mutation.

`worktree_discard_file` bắt buộc confirmation. Tracked path dùng `git restore --worktree` để khôi phục từ index, không đụng staged side. Untracked path dùng `git clean -f` với một backend-resolved literal path, không `-d`, không ignored/all. Token bind fingerprint và fail closed nếu content đổi sau preview. Mọi mutation serialize theo common-dir queue, bump version và phát snapshot/status tokens mới.

Đã triển khai (T18): detached checkout, tag create, history push-to (explicit remote/branch, không force), cherry-pick/revert `--no-commit`, merge commit OID, rebase-onto, reword/modify/edit-author HEAD-scoped, split HEAD, move-to-branch, scripted interactive rebase (plan pick/reword/squash/fixup/drop, bao phủ đúng range), reset soft/mixed/hard (hard có token + gõ lại OID). Chưa triển khai: force push, `branch -D`, stash drop/clear, custom hook runner, amend non-HEAD ngoài interactive rebase. Không có universal undo cho discard/reset đã xác nhận; modal phải nêu hậu quả và mặc định Cancel.
