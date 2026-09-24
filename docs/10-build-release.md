# Development, build and release

**Hiện tại chưa có application scaffold.** T01 phải tạo và kiểm chứng các script dưới đây. Không chạy `pnpm install` ở root trước khi đã có `package.json` phù hợp.

## 1. Environment baseline

Linux Ubuntu 24.04 LTS x86_64, system Git policy ≥2.43, Rust stable và Node LTS tương thích Vite/Svelte/Tauri phiên bản đã chọn. Pin exact toolchain, package manager và lockfiles tại T01; ghi phiên bản thực vào `docs/evidence/environment.md` khi file đó được tạo.

Linux cần compiler/build tools và WebKitGTK dependencies theo hướng dẫn Tauri cho distro. Agent kiểm installed packages trước khi đề xuất cài; không đoán package names hoặc dùng sudo âm thầm. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## 2. Scaffold an toàn trong thư mục đang có tài liệu

1. Kiểm file hiện có, trạng thái Git nếu hợp lệ và các permission/mount restrictions. Thư mục `.git` tồn tại không bảo đảm đây là Git worktree hợp lệ.
2. Chọn template **Svelte + TypeScript** của create-tauri-app hoặc Vite SPA rồi thêm Tauri; scaffold trong staging directory nếu generator đòi folder rỗng.
3. Review generated files, copy từng file cần vào root; giữ `docs/`, `AGENTS.md`, `README.md`, `example.webp`. Không xóa directory hiện tại hoặc ghi đè docs để chiều generator.
4. Tạo scripts, lockfiles và minimal tests, pin versions; record instructions thực tế.
5. Chạy native dev app và preflight command; ghi rõ check pass hoặc blocker, không đánh dấu native success bằng trang Vite.

Tauri cung cấp template Svelte và lựa chọn TypeScript; T01 phải kiểm lệnh/flags đúng phiên bản generator dùng. [Create a project](https://v2.tauri.app/start/create-project/)

Không tự `git init` khi `.git` đã tồn tại nhưng metadata không đọc/ghi được; xác định nguyên nhân trước. Không cần Git init để đọc bộ docs hoặc lập kế hoạch.

## 3. Script contract sau T01

| Lệnh dự kiến | Ý nghĩa | Có từ task |
|---|---|---|
| `pnpm install --frozen-lockfile` | Install đúng lockfile sau lần scaffold đầu | T01 |
| `pnpm dev` | Browser preview, mock adapter explicit và nhãn Demo data | T01/T02 |
| `pnpm tauri dev` | Native app dev, real IPC | T01 |
| `pnpm check` | Svelte + TypeScript check | T01 |
| `pnpm lint` | ESLint và conventions | T01 |
| `pnpm test:unit` | Vitest non-watch | T01 |
| `pnpm test:ui` | Browser interaction tests dùng mock adapter | T02 |
| `pnpm test:visual` | Visual fixture captures/comparisons | T02 |
| `pnpm test:native` | Native E2E runner, real Git fixtures | T15; smoke setup ở T01 |
| `pnpm build` | Production frontend compile | T01 |
| `pnpm tauri build --bundles deb` | Linux installer release | T16 |

Rust checks (sau scaffold):

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Test-only automation features kiểm riêng; không dùng `--all-features` cho release artifact vì có thể bật test server. Không tạo empty scripts trả exit 0 để giả checks pass. Lệnh test chưa được task hiện tại tạo phải ghi `not yet implemented`, không ghi passed.

## 4. CI plan

| Lane | Khi chạy | Nội dung |
|---|---|---|
| Static/unit | Mỗi PR/change review | TS/Svelte/lint/Vitest, Rust fmt/clippy/tests, DTO parity |
| Integration | Mỗi PR thay Git engine | Disposable repos, path/race/network-local tests |
| Browser visual | UI changes | 3 states × 2 sizes, keyboard and error states |
| Linux native | Main/PR milestone | Native app + Git E2E trên baseline, test-only feature config |
| Package | Release candidate | Production .deb, checksum, clean install smoke, no test features |

CI service là lựa chọn triển khai tại T01 khi biết repository hosting; chưa giả project đã có GitHub remote. Có thể dùng local scripts trước, sau đó chuyển sang GitHub Actions nếu repo ở GitHub. Build logs không chứa secret paths/tokens. Cache dependencies theo lockfile/platform; không cache mutable test repositories.

## 5. Release checklist

1. T15 evidence đủ, source state sạch/được ghi nhận, versions pin, license/branding decision đã được chủ project xác nhận trước public release.
2. Build production bundle không có mocks/test server; scan permissions/config; lưu SHA-256 checksum, app version, source commit hoặc snapshot ID thật.
3. Cài .deb trên baseline sạch; chạy launcher, Git preflight, open/stage/commit/remote-local, restart settings; uninstall không xóa Git repos hay app data âm thầm.
4. Viết release notes: features, Git requirement, supported distro/architecture, known limitations, install/uninstall và rollback app binary. App rollback không rollback Git history.
5. Bàn giao artifact local và notes. Publish release/update server chỉ khi người dùng yêu cầu; signed updater và Windows/macOS packaging nằm P2.

MVP ưu tiên .deb; AppImage là follow-up tùy kết quả dependency/portability smoke, không là release blocker nếu chưa nhận scope. Windows/macOS cần matrix paths, credentials, native tests và signing riêng; Rust/Tauri portability không tự bảo đảm chức năng đã verified trên các OS đó.
