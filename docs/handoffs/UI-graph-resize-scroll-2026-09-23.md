# Handoff — Graph resize và cuộn ngang

- Ngày: 2026-09-23. Follow-up T05 theo lỗi user báo: main panel graph không resize/cuộn ngang được.
- Implementation, frontend checks, browser QA và native packaging hoàn tất. Native UI acceptance blocked/not run do phiên chỉ có browser automation.

## Nguyên nhân và thay đổi

1. `HistoryPane.svelte` ép Graph tối thiểu bằng toàn bộ lanes; fixture 24 lanes bị khóa ở 496px. Nay column là viewport 64–1600px, graph content giữ lane spacing 20px. Thanh cuộn dưới nhãn Graph dịch SVG của tất cả rows cùng nhau; keyboard và selection reveal hỗ trợ lane ngoài vùng nhìn thấy.
2. Header trước đây dùng một div overflow riêng + transform theo scrollLeft của rows, nên focus separator có thể tự cuộn header mà rows không đổi. Header sticky chuyển vào cùng scroll container với rows. Virtual window và reveal tính thêm 32px header. Scrollbar ngang luôn có chỗ ở đáy main panel.
3. `ColumnResize.svelte` mở rộng hit area 10px, nằm trong mép cột để không tạo overflow giả. `Splitter.svelte` dùng delta clientX thay movementX, chỉ nhận nút trái, chặn text selection/touch pan, dọn dragging khi pointer cancel/lost capture. Kéo sidebar/inspector đổi kích thước main panel.

## Kiểm chứng

- 65 frontend tests pass, check 0 errors/0 warnings, lint và production build pass.
- `pnpm tauri build` pass trên source cuối: executable + `.deb`/`.rpm`/`.AppImage`. [Artifact checksums](../evidence/UI-graph-resize-scroll-2026-09-23/sha256.txt), [source checksums](../evidence/UI-graph-resize-scroll-2026-09-23/source-sha256.txt). Binary: `src-tauri/target/release/gitdock`; các gói trong `src-tauri/target/release/bundle/`.
- Browser: fixture 10k commits/24 lanes, mouse/keyboard column resize, lane scroll và main scroll, header alignment, 26 DOM rows khi chọn oldest, hide/show giữ viewport.
- App demo 1280×800/1440×900: kéo hai panel, scroll ngang, mở/đóng diff giữ offset, search results/empty và reset widths pass. Console sạch.
- Rust/IPC không đổi; baseline 118 Rust tests ở lượt multi-repo, không ghi nhận như check chạy lại lần này.
- [QA chi tiết](../evidence/UI-graph-resize-scroll-2026-09-23/qa.md), [native build](../evidence/UI-graph-resize-scroll-2026-09-23/native-build.log).

## Nghiệm thu còn lại

Native UI thật **blocked/not run** vì không có native window automation. T15/T16 partial và M1–M4 chưa đổi trạng thái. Không commit/push/install/publish; source còn untracked.

Thao tác tiếp theo (ước lượng 1 phút): mở binary mới → kéo mép phải tiêu đề Graph → kéo thanh cuộn đáy main panel; với nhiều lanes, thử thanh cuộn ngay dưới nhãn Graph.
