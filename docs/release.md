# Release

[Release Octopus](../.github/workflows/release.yml) creates an official release for every push to `main`, including merge, squash, rebase, and direct pushes. It can also be started manually from Actions → Release Octopus → Run workflow → `main`. Other branches are skipped.

## Pipeline

1. `checks` invokes [CI](../.github/workflows/ci.yml): release tooling, Svelte/TypeScript, ESLint, Vitest, and Rust fmt/clippy/tests. A failed check prevents the build and publication jobs from running.
2. `build` runs on Ubuntu 24.04 x86_64, installs dependencies, stamps the version inside the CI checkout, then runs `pnpm tauri build --ci --bundles deb,rpm,appimage -- --locked`. All three packages and `SHA256SUMS` are required. The Actions artifact is retained for 14 days.
3. `publish` uses `GITHUB_TOKEN`, creates a draft at the exact source SHA, uploads the assets, and publishes it with `draft=false` and `prerelease=false`. Only this job has `contents: write`; no PAT is required. Repository or organization policy must allow the declared workflow permission.

## Versioning and reruns

The version is `MAJOR.MINOR.GITHUB_RUN_NUMBER`: source `0.1.0` plus run 42 becomes `0.1.42` with tag `v0.1.42`. Major and minor come from source; the patch number is replaced by the run number. Failed runs may leave gaps. Do not bump the patch version manually for a release.

[prepare-release.py](../scripts/prepare-release.py) synchronizes package.json, the Tauri configuration, the Cargo manifest, and the local package entry in Cargo.lock. It does not change dependency locks or commit/push a version bump. When starting a new release line, update major/minor consistently in all four files. Do not reset the workflow run counter within the same release line, because that can create duplicate tags.

Every merge receives its own version, and the workflow does not cancel queued runs. `make_latest=legacy` lets GitHub choose Latest using creation time and semantic version when builds finish out of order.

[publish-release.sh](../scripts/publish-release.sh) leaves an already published release unchanged on rerun, resumes an incomplete draft upload, and rejects a tag or release that points to another commit. Permission and network failures are not treated as a missing release. Build and upload failures are not ignored. After resolving a failure, use Re-run failed jobs in Actions.

## Packages

| File | Purpose |
|---|---|
| `Octopus_VERSION_amd64.deb` | Debian/Ubuntu |
| `Octopus-VERSION-1.x86_64.rpm` | RPM-based distributions |
| `Octopus_VERSION_amd64.AppImage` | Portable bundle; requires executable permission |
| `SHA256SUMS` | Verify integrity with `sha256sum --check SHA256SUMS` |

The baseline is Ubuntu 24.04 x86_64 with system Git ≥ 2.43. This workflow does not yet include Windows/macOS, signing, or an updater. Missing native workflow coverage is documented in [development.md](development.md#verification-status); release notes link to the documentation at the exact source commit.

## Local build and verification

See the basic setup in the [README](../README.md#run-from-source). Building all three bundle formats on Ubuntu 24.04 also requires `patchelf`, `xdg-utils`, `file`, and `libfuse2t64`, as installed by the workflow.

```sh
python3 -B tests/release/check-release.py
bash -n scripts/publish-release.sh
pnpm tauri build --ci --bundles deb,rpm,appimage -- --locked
```

Tests use temporary repositories/files and a fake `gh`; they do not publish a real release. If a rebuild fails with `Text file busy`, close the executable in `src-tauri/target/release/` first. Do not enable test-only features in a release build.

The first GitHub-hosted build and publication remain **not run** until the files are pushed to GitHub and the workflow is triggered.
