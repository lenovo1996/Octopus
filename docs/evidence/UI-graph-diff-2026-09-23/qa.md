# Graph / diff UI QA — 2026-09-23

## Môi trường và giới hạn

- Linux workspace, Vite localhost:1422; Codex in-app browser, mock adapter có nhãn Demo data.
- Browser harness ban đầu không kết nối được, nên dùng in-app browser. Cổng 1420 có preview khác; lần kiểm tra này dùng dev server riêng 1422.
- Screenshot đã xem trực tiếp trong phiên: graph 1280×800, diff 1280×800, graph/commit details 1440×900. Không có PNG lưu trong thư mục này.
- Không dùng repository thật làm mutation fixture. Rust integration tests sử dụng disposable repos như suite hiện có.

## Kết quả browser

| Kiểm tra | Kết quả thực tế |
|---|---|
| 1280×800, click Unstaged `src/app/App.svelte` | Diff chiếm main panel: x=221, y=92, width=678, height=684. Không overflow ngang cả trang; inspector/file list vẫn hiện. |
| Chuyển cùng file sang Staged | Header đổi thành `Staged · HEAD → index`; selection chỉ thuộc đúng nhóm. |
| Click × | Diff đóng; Commit history hiện trở lại. |
| Merge commit → file | Header `Commit c9f1a2b3 · parent 1`; unified text hiển thị. |
| Chọn Parent 2 | Diff cũ đóng. Click lại file mở header parent 2. |
| Escape trong lúc Parent 2 đang loading | Graph trở lại, tab Commit details vẫn selected; response không mở lại diff. |
| 1440×900 | Graph/details giữ ba cột; author/commit metadata hiện khi panel đủ rộng. |
| Browser console | Không error/warn trong kiểm tra app. |

## Graph fixture

Mở `/tests/ui/graph.html` bằng Vite. Fixture tách khỏi entry production; 10.000 commits, merge fan-out 24 parents, container rộng 500/1000px.

- Lần đầu: **26 listitems**, clientWidth 483px, scrollWidth 756px. Tâm circle và tâm row sai lệch dưới 1px trên tất cả rendered rows.
- ArrowRight: scrollLeft **273px**, header transform **translateX(-273px)**.
- Select oldest: scrollTop **319432px**, commit-9999 selected và hiện trong virtual window.
- Toggle panel đóng/mở, rồi Toggle width: selection commit-9999 giữ nguyên; scrollTop đổi thành 319417px do scrollbar ngang biến mất ở width lớn. **27 rows** sau ResizeObserver ổn định.
- ArrowUp/ArrowUp/Enter: commit-9998 selected; 27 rows được render.

## Automated checks

- `pnpm check`: 0 errors, 0 warnings.
- `pnpm lint`: exit 0.
- `pnpm test:unit`: 57/57, 13 files.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml`: 113/113 Rust tests, exit 0.
- `pnpm build`: exit 0.
- `pnpm tauri build --no-bundle`: exit 0; release binary tại `src-tauri/target/release/gitdock`, build Rust 2 phút 07 giây.
- Regression tests mới: parent connectivity, adjacent-row continuity, every page split, carried-lane collision, exact diff source/token, late success/error, close/repo switch, retry parent comparison.

Native visual/E2E cho fix mới: **blocked/not run**; browser evidence không xác nhận real Tauri IPC/native paint.
