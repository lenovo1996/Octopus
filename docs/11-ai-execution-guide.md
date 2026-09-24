# AI execution guide

**Dùng [00-start.md](prompts/00-start.md) để giao task đầu tiên.** Bộ tài liệu này được thiết kế để một coding agent làm từng task với context nhỏ, không yêu cầu agent đọc mọi chi tiết trước mọi thay đổi.

## 1. Reading map

| Vai trò / task | Bắt buộc đọc |
|---|---|
| Mọi task | Root AGENTS, README, progress, task definition, quyết định liên quan |
| UI | PRD + UI spec + ảnh gốc + wireframes/tokens + DTOs dùng trong task |
| Git/backend | Architecture + IPC + Git engine + security + QA fixtures liên quan |
| QA/release | PRD traceability + QA + build/release + handoffs đã có |
| Thay đổi phạm vi | Overview + PRD + ADR/risk register, rồi cập nhật docs bị tác động |

Không đọc toàn repo tùy tiện khi đã có task scope. Nếu CodeGraph index tồn tại, dùng nó trước để định vị code theo AGENTS.

## 2. Cách giao việc

1. Chọn task `todo` có tất cả dependencies `done`; T01 là đầu tiên.
2. Dán prompt `01-execute-task.md`, thay task ID. Nếu giao một milestone, nêu cụ thể khoảng task và yêu cầu gate cuối.
3. Agent triển khai, tự kiểm tra, sửa lỗi trong scope, cập nhật progress và handoff.
4. Chạy review prompt, xử lý blocker/P1 trước khi giao task phụ thuộc. Người review có thể là cùng agent ở lượt riêng hoặc agent khác khi user chủ động yêu cầu.
5. Sau milestone, chạy demo/gate của [plan](06-implementation-plan.md); chưa pass gate thì không gọi milestone hoàn tất.

Mặc định không tự chia nhiều agents. Nếu user yêu cầu parallel work, chỉ song song các task độc lập theo graph; chỉ định ownership files, freeze DTO version và một integration owner. Không coi task đã done chỉ vì một agent gửi “finished”.

## 3. Definition of ready

- Task có mục tiêu/acceptance/allowed scope và dependencies đủ.
- Spec/DTO liên quan không mâu thuẫn; nếu có, agent nêu chỗ khác và giải quyết trước code phụ thuộc.
- Có fixture hoặc cách kiểm chứng trực tiếp; operation destructive không dùng repo thật làm test.
- Environment blocker được biết; task không dựa vào credentials, package publishing hoặc permissions chưa có.

## 4. Definition of done

- Implementation đáp ứng acceptance và PRD liên quan; không có fake production success.
- Meaningful checks đã chạy và pass; output hoặc path evidence ghi lại; skipped/blocked phân biệt rõ.
- UI state/error/keyboard và safety behavior của task được kiểm chứng.
- Docs/contracts cập nhật khi thay đổi; progress và handoff đủ để agent tiếp theo tiếp tục.
- Không secret, unsolicited dependency, unrelated changes hay destructive action thiếu xác nhận.

## 5. Progress protocol

Trạng thái hợp lệ: `todo`, `in_progress`, `blocked`, `in_review`, `done`. Mỗi task chỉ một dòng canonical trong progress. `blocked` phải có cause + action unblock; không gắn lỗi môi trường thành product defect nếu chưa có bằng chứng. Không dùng % completion do agent tự ước đoán.

Handoff tại `docs/handoffs/Txx.md`, dùng [template](templates/handoff.md). Evidence tại `docs/evidence/Txx/` khi có: screenshots, short test summary, benchmark JSON/text. Không commit dependency caches, huge raw logs hoặc nội dung repo người dùng.

Report cho người dùng tối đa 5 ý: task/mốc, điều gì chạy, kiểm tra, blocker/risk thực tế, một next action <2 phút. Giao tiếp bằng tiếng Việt; identifiers/source giữ nguyên.

## 6. Xử lý vấn đề

Routine decisions như tên component, utility function, test helper do agent tự làm trong spec. Hỏi một câu ngắn khi thiếu quyết định làm thay đổi scope/hành vi. Sau ba lần sửa cùng lỗi thất bại, dừng loop, nêu assumption đáng nghi và evidence. Không đổi stack, disable tests hoặc xóa file để làm lỗi biến mất.

Nếu không thể chạy native UI, vẫn có thể hoàn thiện phần độc lập nhưng phải ghi native gate `blocked`; không tuyên bố “app chạy” từ `pnpm build` đơn thuần. Review requirements trước khi đề xuất feature mới.
