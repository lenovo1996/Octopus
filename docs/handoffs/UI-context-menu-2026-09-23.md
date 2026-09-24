# Handoff — Context menu

- Ngày: 2026-09-23. Yêu cầu trực tiếp của user: thiết kế và implement menu context khi click chuột phải.
- Implementation, frontend checks, browser QA và native packaging hoàn tất. Giữ stack/dependencies hiện tại; commit menu mở rộng theo ảnh user nhưng chỉ bật action có typed backend flow.

## Thay đổi

1. `ContextMenu.svelte` là menu dùng chung: fixed position, clamp 8px trong viewport, nhóm separator, disabled/danger state, current item, tinted shadow nhỏ. Arrow Up/Down, Home/End, Enter/Space, Escape/Tab và outside click đầy đủ.
2. Repository tabs: Switch, Copy repository path, Open another repository, Close tab. Busy/opening khóa Close và Open phù hợp.
3. History commits: đủ 17 Git actions theo nhóm trong ảnh, sau đó Open commit details, Show in graph trong search, Copy commit ID và Copy subject. `Create branch here…` truyền exact row OID vào typed `branch_create`; modal ghi rõ short OID nguồn và vẫn cho chọn Switch after create.
4. Working changes: Open diff, Stage/Unstage exact pathId+source, Select/Deselect source-specific selection, Copy relative/original path. Commit details files: Open diff và Copy path.
5. Chuột phải và Context Menu/Shift+F10 mở cùng UI. `context-menu/model.ts` giữ positioning/navigation thuần để test độc lập.
6. 16 Git actions chưa có typed command (`checkout`, tag, historical push, cherry-pick/revert, merge/rebase, amend/split/move và reset) hiện `Planned`, disabled và có tooltip nêu contract an toàn còn thiếu. Hard reset có danger color. Menu tự scroll nếu cao hơn viewport.
7. Regression QA bắt lỗi `TopBar` truyền `MouseEvent` vào callback có tham số OID, làm modal gọi `.slice()` trên event sau mutation. Đã tách `openBranches()` và `openBranchesAt(oid)`; TopBar luôn dùng HEAD, commit menu chỉ nhận string OID.

## Kiểm chứng và giới hạn

- `pnpm check` sạch; lint, 72 tests/17 files và production build pass.
- Browser @1280×720: đủ 17 labels/group separator, planned state và hard-reset danger pass. Regression cuối kiểm tra TopBar mở từ HEAD; Create branch here trên `e5f6071`, bỏ Switch after create rồi tạo `qa/regression`: ref mới trỏ đúng `e5f6071`, HEAD vẫn `main`, console sạch.
- Final `pnpm tauri build` ngoài sandbox exit 0, tạo executable + `.deb`/`.rpm`/`.AppImage`. Lần sandbox trước đó lỗi tại linuxdeploy vì cần tải AppImage runtime. [Artifact checksums](../evidence/UI-context-menu-2026-09-23/sha256.txt), [source checksums](../evidence/UI-context-menu-2026-09-23/source-sha256.txt).
- Browser @1280×800: menu tab/commit/file, keyboard-only flow, viewport edge, Open details/Open diff và mock Stage pass; console sạch. [QA chi tiết](../evidence/UI-context-menu-2026-09-23/qa.md).
- Rust/IPC không đổi; baseline trước là 118 Rust tests. Native visual/real-repo workflow **blocked/not run** do không có native window automation; M1–M4 không đổi.
- Không commit/push/install/publish. Source vẫn untracked trên `main`.

Thao tác tiếp theo (ước lượng 1 phút): mở binary mới, chuột phải commit rồi file trên repo thật; kiểm tra Open diff và Stage/Unstage đúng source.
