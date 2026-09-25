# Release

[Release Octopus](../.github/workflows/release.yml) tạo release chính thức cho mỗi lần push vào `main`, gồm merge, squash, rebase và direct push. Có thể chạy thủ công trong Actions → Release Octopus → Run workflow → `main`. Branch khác sẽ skip.

## Pipeline

1. `checks` gọi [CI](../.github/workflows/ci.yml): release tooling, Svelte/TypeScript, ESLint, Vitest, Rust fmt/clippy/tests. Check fail thì không build/publish.
2. `build` trên Ubuntu 24.04 x86_64 cài dependencies, stamp version trong checkout CI, rồi chạy `pnpm tauri build --ci --bundles deb,rpm,appimage -- --locked`. Phải có đủ ba packages và `SHA256SUMS`; Actions artifacts giữ 14 ngày.
3. `publish` dùng `GITHUB_TOKEN`, tạo draft tại đúng source SHA, upload assets rồi publish với `draft=false`, `prerelease=false`. Chỉ job này có `contents: write`; không cần PAT. Repository/organization phải cho phép quyền workflow đã khai báo.

## Version và chạy lại

Version là `MAJOR.MINOR.GITHUB_RUN_NUMBER`: source `0.1.0` + run 42 → `0.1.42`, tag `v0.1.42`. Major/minor lấy từ source; patch thay bằng số run. Failed runs có thể để lại khoảng trống. Không bump patch thủ công để release.

[prepare-release.py](../scripts/prepare-release.py) đồng bộ package.json, Tauri config, Cargo manifest và local package entry trong Cargo.lock; không đổi dependency lock hoặc commit/push version bump. Khi mở dòng phát hành mới, cập nhật major/minor đồng bộ ở bốn file. Không reset run counter trong cùng dòng version để tránh trùng tag.

Mỗi merge có version riêng; workflow không hủy các runs đang chờ. `make_latest=legacy` để GitHub chọn Latest theo ngày tạo và semantic version khi builds kết thúc khác thứ tự.

[publish-release.sh](../scripts/publish-release.sh) giữ nguyên release đã publish khi rerun; tiếp tục upload draft dở; từ chối tag/release trỏ commit khác. Lỗi quyền/network không được hiểu là release chưa tồn tại. Build/upload lỗi không được bỏ qua. Sau khi xử lý lỗi, dùng Re-run failed jobs trong Actions.

## Packages

| File | Mục đích |
|---|---|
| `Octopus_VERSION_amd64.deb` | Debian/Ubuntu |
| `Octopus-VERSION-1.x86_64.rpm` | RPM-based distributions |
| `Octopus_VERSION_amd64.AppImage` | Portable bundle; cần quyền executable |
| `SHA256SUMS` | Kiểm tra integrity bằng `sha256sum --check SHA256SUMS` |

Baseline là Ubuntu 24.04 x86_64 và system Git ≥ 2.43. Workflow này chưa gồm Windows/macOS, signing hoặc updater. Các native workflow còn thiếu được ghi trong [development.md](development.md#trạng-thái-kiểm-chứng); release notes liên kết tài liệu tại đúng source commit.

## Build và kiểm tra local

Setup cơ bản: [README](../README.md#chạy-từ-source). Để bundle cả ba định dạng trên Ubuntu 24.04, cần thêm `patchelf`, `xdg-utils`, `file`, `libfuse2t64` như workflow.

```sh
python3 -B tests/release/check-release.py
bash -n scripts/publish-release.sh
pnpm tauri build --ci --bundles deb,rpm,appimage -- --locked
```

Tests dùng repos/files tạm và `gh` giả; không publish thật. Đóng executable trong `src-tauri/target/release/` trước khi build lại nếu gặp `Text file busy`. Không bật test-only features trong release.

Lần GitHub-hosted build/publish đầu tiên còn **chưa chạy** cho đến khi files được đưa lên GitHub và trigger workflow.
