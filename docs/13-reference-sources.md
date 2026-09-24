# Reference sources

Các nguồn chính thức đã kiểm tra ngày **2026-09-22**. Tài liệu là thiết kế dự án; các budgets, component names, task estimates và phạm vi release là đề xuất riêng, không phải claim từ nguồn.

## Product reference

[example.webp](../example.webp) do người dùng cung cấp: basis cho top toolbar, sidebar, graph center và ba inspector states. Không xác định tên sản phẩm gốc, license ảnh hay exact color/font từ ảnh mờ; dùng làm visual reference nội bộ, không đóng gói watermark/branding vào sản phẩm.

## Frameworks

| Nguồn | Dùng để xác minh |
|---|---|
| [Tauri frontend configuration](https://v2.tauri.app/start/frontend/) | Vite SPA/static frontend, không cần SSR server |
| [Tauri create project](https://v2.tauri.app/start/create-project/) | Template Svelte/TypeScript và scaffold flow |
| [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) | Native Linux build dependencies theo distro |
| [Tauri commands](https://v2.tauri.app/develop/calling-rust/) | Async Rust commands, serialized request/response |
| [Tauri capabilities](https://v2.tauri.app/security/capabilities/) | Permission scope và custom-command default access |
| [Tauri WebDriver](https://v2.tauri.app/develop/tests/webdriver/) | WDIO embedded cross-platform và direct-driver limitations |
| [Svelte runes](https://svelte.dev/docs/svelte/what-are-runes) | Reactivity API cho Svelte 5 |

## Git

| Nguồn | Dùng để xác minh |
|---|---|
| [Git global options](https://git-scm.com/docs/git) | Literal pathspec handling và command configuration |
| [Git status](https://git-scm.com/docs/git-status) | Porcelain v2, NUL paths, index/worktree fields |
| [Git rev-list](https://git-scm.com/docs/git-rev-list) | Parent traversal/topological order |
| [Git diff](https://git-scm.com/docs/git-diff) | Diff variants, binary, external diff/textconv controls |
| [Git merge](https://git-scm.com/docs/git-merge) | --no-commit/--no-ff, conflict/abort semantics |
| [Git stash](https://git-scm.com/docs/git-stash) | Apply/pop/conflict and stash retention |
| [Git attributes](https://git-scm.com/docs/gitattributes) | Clean/smudge/process filters và content conversion trust boundary |

Trước pin dependency hoặc thêm primitive Git chưa được fixture kiểm chứng, agent đọc lại nguồn chính thức đúng phiên bản. Không dùng search snippet hoặc blog thay acceptance test thực tế.
