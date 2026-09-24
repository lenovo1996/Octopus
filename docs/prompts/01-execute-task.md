# Prompt giao một task

Thay `Txx` bằng task ID, rồi gửi nội dung dưới cho coding agent.

---

Triển khai **Txx** của GitDock trong project hiện tại. Đọc AGENTS.md, README.md, docs/progress.md, task definition trong docs/07-task-backlog.md và specs mà task tham chiếu. Nếu có codegraph index, tuân thủ CodeGraph workflow trước khi đọc/tìm code.

Kiểm dependencies đã done bằng evidence, không chỉ nhãn. Nêu plan tối đa 5 bước rồi implement và kiểm chứng trong phạm vi task. Bảo toàn thay đổi có sẵn, không đổi stack hoặc tự mở rộng sang P2. UI phải theo example.webp + docs/02-ux-ui-spec.md + docs/design; Git behavior phải theo IPC/Git engine/security.

Dùng disposable repositories cho mutation tests; không mock Git thành công trong production. Chạy checks phù hợp; ghi đúng passed/failed/blocked/not-run. Routine decisions tự giải quyết theo spec. Nếu mâu thuẫn scope thực sự, hỏi một câu ngắn và tiếp tục phần độc lập. Sau ba fix thất bại cùng lỗi, dừng loop và nêu assumption đáng nghi.

Cập nhật progress và docs/handoffs/Txx.md. Báo kết quả ngắn bằng tiếng Việt: task/gate, thay đổi đã chạy được, bằng chứng kiểm tra, giới hạn, next action <2 phút. Chưa tự làm task kế tiếp nếu lần giao này chỉ nêu Txx.
