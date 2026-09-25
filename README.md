# Octopus

Git GUI desktop cho Linux, dùng Rust + Tauri 2 + Svelte 5 + TypeScript. Mở nhiều repository, xem commit graph/diff, stage và commit, quản lý branch/stash, merge và đồng bộ remote.

## Chạy từ source

Cần system Git ≥ 2.43, Node theo [.nvmrc](.nvmrc), pnpm theo `packageManager` trong [package.json](package.json), Rust theo [rust-toolchain.toml](rust-toolchain.toml).

Trên Ubuntu 24.04, cài dependencies native:

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev libxdo-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` mở browser demo bằng dữ liệu giả. Các thao tác Git thật chạy trong desktop app.

## Kiểm tra và build

```sh
pnpm check
pnpm lint
pnpm test:unit
python3 -B tests/release/check-release.py
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

```sh
pnpm build
pnpm tauri build --bundles deb -- --locked
```

Frontend output nằm trong `dist/`; executable và packages nằm trong `src-tauri/target/release/`. Browser fixtures ở `tests/ui/`; chưa có runner UI/visual/native E2E tự động.

## Cấu trúc

| Đường dẫn | Nội dung |
|---|---|
| `src/app/`, `src/lib/` | Svelte UI, state, typed IPC và styles |
| `src/mocks/`, `tests/` | Browser demo, unit tests, UI fixtures và release tests |
| `src-tauri/` | Rust Git engine, commands, persistence, Tauri config và native icons |
| `public/` | Icon gốc, icon UI và favicon |
| `.github/workflows/`, `scripts/`, `docs/` | CI/release, scripts và hướng dẫn bảo trì |

## Release

Merge/push vào `main` chạy checks, build Linux `.deb`, `.rpm`, `.AppImage`, rồi tạo **GitHub Release chính thức** kèm `SHA256SUMS`. Version là `MAJOR.MINOR.GITHUB_RUN_NUMBER`; workflow chỉ stamp version trong checkout CI.

Xem [release workflow](.github/workflows/release.yml), [hướng dẫn release](docs/release.md) và [trạng thái kiểm chứng](docs/development.md#trạng-thái-kiểm-chứng).

## Branding và dữ liệu cũ

Tên app/executable là **Octopus** / `octopus`. [Icon bạch tuộc gốc](public/brand/octopus.png) được dùng để tạo native icons, icon UI và favicon; xem [cách tái tạo](docs/development.md#icon).

Giữ `com.gitdock.app`, `gitdock-settings.json`, storage keys `gitdock.*` và IPC events `gitdock://*` để tương thích dữ liệu GitDock đã lưu. Các tên nội bộ này không phải lỗi đổi tên.

Hướng dẫn đóng góp: [AGENTS.md](AGENTS.md) và [development.md](docs/development.md). License: [MIT](LICENSE-MIT).
