# Environment baseline (recorded at T01 scaffold)

**Date:** 2026-09-22. **Machine:** Ubuntu 24.04.4 LTS x86_64.

| Tool | Version | Source |
|---|---|---|
| OS | Ubuntu 24.04.4 LTS, x86_64 | `lsb_release -a`, `uname -m` |
| System Git | 2.43.0 (meets policy ≥ 2.43) | `git --version` |
| Rust | 1.98.1 (pinned via `rust-toolchain.toml`) | `rustc --version` |
| Node | v24.15.0 (pinned via `.nvmrc`) | `node --version` |
| pnpm | 10.34.5 (pinned via `packageManager`) | `pnpm --version` |
| create-tauri-app | 4.7.4 (staging scaffold only) | `npx create-tauri-app --version` |
| Network | npm registry reachable (PONG 477ms) | `npm ping` |

## Frontend pins (`package.json`, exact)

svelte 5.57.1 · vite 8.3.0 · @sveltejs/vite-plugin-svelte 7.3.0 ·
typescript 6.0.3 · svelte-check 4.7.6 · vitest 5.0.1 · eslint 10.11.0 ·
typescript-eslint 8.70.1 · @eslint/js 10.0.1 · @tauri-apps/api 2.11.1 ·
@tauri-apps/cli 2.11.5 · @tauri-apps/plugin-opener 2.5.5.

Lockfile `pnpm-lock.yaml` is authoritative after install.

## Rust pins (`src-tauri/Cargo.toml` + `Cargo.lock`)

tauri 2 · tauri-plugin-opener 2 · serde 1 · serde_json 1 · thiserror 2 ·
tokio 1 (rt-multi-thread, macros, process, time) · tracing 0.1 · uuid 1 (v4).
Exact versions land in `Cargo.lock` on first build.

## System dependencies

`libwebkit2gtk-4.1-dev` 2.52.6 and `libjavascriptcoregtk-4.1-dev` present;
`build-essential`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`
present. No extra `sudo` installs were needed.

## Notes

- Workdir has no `.git` (progress.md's old "`.git` present but invalid" note no
  longer applies); scaffold copied files from a staging dir, docs untouched.
- Generator templates were SvelteKit-based, so the frontend was hand-built as a
  plain Vite SPA per docs/03-architecture.md; only `src-tauri` came from the template.
