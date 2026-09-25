# Hướng dẫn cho AI triển khai Octopus

## CodeGraph

Trong repository có `.codegraph/` tại root, dùng CodeGraph trước khi grep/find hoặc đọc code để tìm hiểu hay định vị implementation. Dùng MCP `codegraph_explore` nếu có, hoặc `codegraph explore "<symbol names or question>"`. Nếu không có `.codegraph/`, bỏ qua; không tự index.

## Đọc trước khi sửa

1. Đọc `README.md` và `docs/development.md`; với build/release, đọc thêm `docs/release.md`.
2. Với UI, xem màn hình đang chạy và `src/lib/styles/`; dùng icon trong `public/` và `src-tauri/icons/`.
3. Với backend, đọc typed contracts ở `src/lib/ipc/types.ts`, Rust DTOs/commands và Git engine liên quan; giữ các ràng buộc Git/dữ liệu trong `docs/development.md`.
4. Xác nhận dependency của task đã hoàn thành; chỉ sửa phần cần thiết cho task hiện tại.

## Ràng buộc implementation

- Rust + Tauri 2 + Svelte 5 + TypeScript strict + Vite; Linux trước. Không tự đổi stack, thêm server, tài khoản cloud hoặc AI runtime vào sản phẩm.
- Svelte 5 runes cho state mới. UI không gọi shell, không giữ credentials, không tự coi state cache là Git truth.
- Mọi thao tác Git qua typed command và Rust Git engine. Không tạo API `run_command(string)` hoặc nhận argv tùy ý từ frontend.
- Không tự mở rộng phạm vi feature. Không biến mục chưa hỗ trợ thành nút bấm giả.
- Dùng repo tạm cho test mutation. Không dùng repository thật của người dùng làm test fixture.
- Không chạy `reset --hard`, `clean`, force push, xóa file/repo hoặc thay đổi lịch sử người dùng khi chưa có xác nhận cụ thể. Không tự commit/push/publish thay người dùng.
- Không chạy generator ghi đè project hiện có để khắc phục lỗi scaffold.
- Không tự spawn subagent. Chỉ chia việc cho agent khi người dùng yêu cầu; mặc định một agent thực hiện task tuần tự.

## Definition of done mỗi task

Code đúng acceptance criteria; checks liên quan pass; UI có loading/empty/error nếu áp dụng; không có secret trong logs; cập nhật trạng thái/checks/blocker trong `docs/development.md`. Không tạo thêm evidence/handoff theo từng task. Nếu môi trường không chạy được check, ghi `blocked` cho gate đó và chỉ rõ nguyên nhân. Không tuyên bố đã kiểm chứng thay cho việc chạy thật.

## Output style

Người đọc có ADHD. Viết tiếng Việt, giữ identifiers kỹ thuật bằng tiếng Anh.

1. Dẫn bằng kết quả hoặc hành động tiếp theo: command, path hoặc snippet trước.
2. Đánh số công việc nhiều bước; mỗi bước là một hành động giới hạn.
3. Kết thúc bằng một hành động làm được trong dưới 2 phút.
4. Xử lý xong vấn đề hiện tại trước khi nêu vấn đề mới.
5. Nêu tiến độ mỗi lượt, ví dụ “bước 3/5 hoàn tất”.
6. Ước lượng thời gian bằng phút/giờ/ngày cụ thể; ghi rõ đó là ước lượng.
7. Sau thay đổi, chỉ ra điều gì chạy được và bằng chứng.
8. Với lỗi, ghi vị trí, nguyên nhân và cách sửa.
9. Nhóm và xếp ưu tiên danh sách dài, tối đa khoảng 5 mục mỗi nhóm.
10. Không preamble, recap hay lời kết chung chung. Giải thích đầy đủ khi được yêu cầu. Xác nhận trước hành động phá hủy. Sau ba lần sửa thất bại cùng lỗi, dừng và nêu giả định đang đáng nghi. Nếu yêu cầu mơ hồ, hỏi một câu ngắn.
