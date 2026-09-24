# Prompt khởi động — sao chép toàn bộ phần dưới

Bạn là coding agent triển khai GitDock trong repository hiện tại. Sản phẩm là Git GUI desktop Rust + Tauri 2 + Svelte 5 + TypeScript/Vite, Linux trước, dựa trên example.webp.

Đọc AGENTS.md, README.md, docs/progress.md, docs/00-project-overview.md và task T01 trong docs/07-task-backlog.md. Đọc tiếp architecture, IPC contracts và build/release được task tham chiếu. Xem ảnh example.webp trực tiếp để hiểu hướng UI.

Hãy **thực hiện T01**, không chỉ đưa kế hoạch. Preflight môi trường, xác minh metadata Git và bảo toàn toàn bộ tài liệu/ảnh hiện có. Nếu generator yêu cầu folder rỗng, scaffold ở staging directory rồi đưa các file cần vào root; không xóa project hiện tại. Không tự khởi tạo lại .git không hợp lệ hoặc inaccessible.

Pin phiên bản dependency/toolchain phù hợp; tạo real/mock IPC boundary, command preflight thật, scripts và meaningful tests trong scope. Chạy checks và native launch khi môi trường cho phép. Chỉ ghi pass khi đã chạy thành công; nếu native bị block thì nêu command, cause và bước unblock, hoàn tất phần độc lập có thể làm.

Cập nhật docs/progress.md và tạo docs/handoffs/T01.md theo template. Không làm P2, không tự chạy nhiều agents, không commit/push/publish hoặc thay đổi repo người dùng để test.

Kết thúc bằng: task và gate đạt được, điều gì hiện chạy được, checks/evidence, blocker nếu có, và một hành động tiếp theo dưới 2 phút. Dùng tiếng Việt. Dừng ở ranh giới T01 để kết quả được review trước T02.
