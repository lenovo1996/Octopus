# Prompt review một task/milestone

Thay `Txx hoặc Mx` trước khi gửi.

---

Review **Txx hoặc Mx** của GitDock. Đọc AGENTS.md, PRD, task acceptance, progress/handoff và specs liên quan. Kiểm code/evidence thực tế thay vì dựa vào mô tả của agent trước.

Ưu tiên: (1) mất dữ liệu hoặc mutation sai target; (2) sai Git state/topology/IPC race; (3) thiếu scope hoặc UI behavior; (4) lỗi permissions/secret/rendering; (5) testing/release evidence thiếu. Mỗi finding phải có path:line, tình huống tái hiện, ảnh hưởng và cách sửa đề xuất. Không bịa line hoặc claim đã chạy test.

Chạy read-only checks và tests trên repo tạm nếu cần. Không sửa code trong lượt review này trừ khi được giao thêm. Phân biệt native E2E với browser mock. Kết luận PASS chỉ khi acceptance có evidence; nếu không thì ghi FAIL hoặc BLOCKED kèm gate còn thiếu.

Trả lời tiếng Việt, findings nghiêm trọng trước, không liệt kê nitpick khi còn lỗi chức năng. Kết thúc bằng một hành động sửa hoặc kiểm tra dưới 2 phút để bắt đầu xử lý finding đầu.
