# Graph resize / horizontal scroll QA — 2026-09-23

Phạm vi: browser fixture và app demo qua CUA; không phải native WebKit/Git workflow. Dev server đã chạy sẵn tại localhost:1420; không dừng server của user. Không Git mutation, không cài đặt artifact.

## Tái hiện trước sửa

- `tests/ui/graph.html`: 10,000 commits, 24 lanes, panel 500px.
- Graph có `aria-valuenow=496`, `aria-valuemin=496`. ArrowLeft không thay đổi width. Code ép width tối thiểu bằng laneCount × 20 + 16.
- Focus separator ngoài viewport khiến header cuộn riêng, lệch cột với rows. Viewport clientWidth 483px, scrollWidth 986px.
- Shell splitter dùng `movementX`; đổi sang delta `clientX` cho cùng cơ chế pointer với column resize. Không khẳng định đã tái hiện movementX=0 trên native trong phiên này.

## Sau sửa — thao tác UI thật

1. Graph Home reset 84px; mouse drag +120px → 204px, dưới giới hạn 496px cũ. Shift+Left → 172px. Subject Shift+Right → 232px; Author Right → 148px. Widths được khôi phục khi reload và khi mở workspace khác.
2. Header Graph có native scrollbar khi lanes overflow. End → lane scrollLeft 292, mọi SVG `translateX(-292px)`, main scrollTop vẫn 0. Kéo thumb về trái → offset 0. Các lane giữ khoảng cách và hình học ban đầu.
3. Kéo scrollbar đáy panel fixture → main scrollLeft 211px; header và row đều x=-194px, Author vào viewport. Không còn transform/cached offset riêng cho header.
4. Chọn oldest commit → commit-9999 selected, scrollTop 319439, chỉ 26 row DOM; sticky header y=104. Hide/show panel giữ scrollTop 319439 và scrollLeft 211.
5. App demo 1280×800: kéo inspector từ x=899 tới 799 → main width 678→578px; kéo sidebar +40px → main 538px. Scrollbar nằm ở đáy main panel. Kéo ngang → scrollLeft 164, header/row cùng x=97.
6. Mở unstaged App.svelte → File diff trong main panel; Close diff → graph giữ scrollLeft 164, header/row vẫn x=97.
7. Search `inspector` → 1 result, Subject/Author/Show in graph; query không khớp → empty message; Clear trả graph. App 1440×900: documentWidth 1440, main width 838 sau reset splitters, header/row cùng x=221.
8. Ở commit cuối, kéo Graph 172→532px làm scrollbar lane biến mất, rồi kéo 532→172px để hiện lại: scrollTop giữ 319439, vẫn 26 DOM rows. Effect reset history chỉ phụ thuộc search/scope; mount/unmount scrollbar khi resize không reset viewport.
9. Console app không warning/error tại thời điểm kiểm tra. Screenshots được xem trực tiếp trong CUA; không xuất PNG.

## Checks

- `check.log`: svelte-check 0 errors / 0 warnings.
- `lint.log`: ESLint exit 0.
- `unit.log`: 65 tests / 15 files pass; đây là suite hiện có, hành vi DOM mới kiểm tra bằng browser ở trên.
- `build.log`: production frontend pass.
- `native-build.log`: final source build exit 0; executable + `.deb`/`.rpm`/`.AppImage`. `sha256.txt` ghi artifact hashes; `source-sha256.txt` đối chiếu ba components đã sửa.
- Rust engine/IPC không đổi; không chạy lại Rust suite trong fix này. Baseline trước là 118 pass trong handoff multi-repo.
- Native UI trên repo thật: **blocked/not run**, phiên không có native window automation. Browser demo không nâng milestone gates M1–M4.
