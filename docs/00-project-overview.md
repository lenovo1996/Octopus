# Project overview

## Quyết định nền tảng

| Thuộc tính | Quyết định |
|---|---|
| Tên làm việc | GitDock; chưa chốt thương hiệu, domain hoặc license |
| Sản phẩm | Git GUI desktop cho developer làm việc với repository local và remote |
| Stack | Rust, Tauri 2, Svelte 5, TypeScript strict, Vite |
| Nền tảng | Linux trước; baseline nghiệm thu Ubuntu 24.04 LTS x86_64; Windows/macOS sau MVP |
| Phong cách | Dark, compact, bố cục ba cột theo `example.webp` |
| Ngôn ngữ | Tài liệu tiếng Việt; UI tiếng Anh trước, tách chuỗi để thêm tiếng Việt sau |
| Git backend | System Git CLI qua Rust adapter; không nhúng Git binary ở MVP |
| Trạng thái | Planning ready; mọi task implementation đang `todo` |

Baseline distro, tên, UI language là giả định thiết kế có thể đổi. Linux trước là yêu cầu đã được chủ project xác nhận. Kiến trúc dùng Svelte SPA và Vite phù hợp mô hình frontend tĩnh của Tauri. [Tauri frontend](https://v2.tauri.app/start/frontend/)

## Vấn đề và giá trị

Developer cần đọc lịch sử phân nhánh, xem thay đổi và thực hiện vòng lặp stage → commit → sync trong cùng cửa sổ. GitDock giữ graph là vùng trung tâm và đưa nội dung liên quan vào inspector, giảm việc chuyển giữa màn hình hay terminal cho thao tác hằng ngày.

Ba kết quả người dùng phải đạt được:

1. Mở một repo và hiểu HEAD, nhánh đang làm việc, thay đổi chưa commit.
2. Đọc diff, stage đúng file, tạo commit và kiểm chứng commit mới trên graph.
3. Fetch, pull an toàn, push và xử lý merge conflict với trạng thái rõ ràng.

## Đối tượng sử dụng

- Developer đã biết commit/branch/remote, muốn làm việc trực quan và nhanh bằng bàn phím.
- Người mới với Git cần nhãn hành động rõ ràng, thông báo nguyên nhân lỗi và cách tiếp tục.
- Người dùng nhiều repo cần recent repositories, nhưng MVP chỉ mở một repo trong một cửa sổ tại một thời điểm.

## Ranh giới phát hành

| Mốc | Phạm vi | Điều kiện kết thúc |
|---|---|---|
| Local Alpha | Open/init, graph, refs, status, diff, stage/unstage, commit, branch, search | Workflow local chạy trên repo thật được tạo riêng cho test |
| Linux MVP | Alpha + clone, fetch/pull/push, stash, merge và conflict, preferences, installer | Tất cả P0/P1 đạt QA gate và Linux package smoke pass |
| Sau MVP | Undo/redo có giới hạn rõ, PR integrations, submodule management, rebase, cherry-pick, hunk staging, Windows/macOS | RFC/ADR bổ sung và acceptance criteria riêng |

Không đưa AI vào ứng dụng ở MVP. “Assign AI” nghĩa là dùng coding agent để xây sản phẩm, không phải tính năng sinh commit message bằng AI.

## Thước đo thành công

Các con số dưới đây là **mục tiêu cần đo**, không phải benchmark hiện có.

| Mục tiêu | Cách đo |
|---|---|
| Vòng lặp local hoàn chỉnh | Test open → edit → diff → stage → commit; Git CLI kiểm tra tree và message |
| Git state chính xác | UI khớp HEAD, index, worktree, refs trong fixture thường và các edge case |
| Giữ được bố cục tham chiếu | Review tại 1440×900 và 1280×800 theo UI rubric |
| Đáp ứng tốt | Mốc 10k commits/5k files: first usable ≤2 giây warm, ≤5 giây cold; đo trên máy chuẩn được ghi lại |
| An toàn dữ liệu | Không overwrite thay đổi ngoài ý muốn; stale request bị từ chối; không tự force/clean/reset |

## Giới hạn ban đầu

Git đã cài, policy minimum 2.43; agent xác minh trên baseline khi scaffold. Không hỗ trợ bare repo để làm việc, partial/promisor clones, mobile, git-annex, quản lý Git LFS riêng, hoặc merge octopus qua UI. Shallow clones có đủ objects được hỗ trợ. Repo chứa submodule/LFS vẫn cần hiển thị trạng thái hoặc cảnh báo đúng; không âm thầm sửa dữ liệu không hỗ trợ.

**Bước tiếp theo:** đọc [PRD](01-product-requirements.md), sau đó bắt đầu `T01` theo [backlog](07-task-backlog.md).
