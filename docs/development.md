# Development

## Architecture

The app uses a Vite SPA, Svelte 5 runes, and strict TypeScript. `src/app/` composes the workspace, `src/lib/components/` contains the UI, and `src/lib/styles/tokens.css` is the single source of design tokens.

The frontend calls typed IPC through [client.ts](../src/lib/ipc/client.ts), selecting the [real adapter](../src/lib/ipc/real.ts) inside Tauri or the [mock adapter](../src/lib/ipc/mock.ts) for the browser demo. Data contracts live in [types.ts](../src/lib/ipc/types.ts) and the Rust DTOs under [domain](../src-tauri/src/domain/) and [commands](../src-tauri/src/commands/). When changing a payload, update both sides and verify JSON serialization.

The backend is divided into `commands/` for IPC handlers, `git/` for system Git, `services/` for the session/job registry, `domain/` for DTOs and errors, and `persistence/` for settings. The [lib.rs](../src-tauri/src/lib.rs) entry point registers commands. Do not expose a shell or an API that accepts arbitrary argv to the frontend.

## Git and data

- Treat repository contents, filenames, configuration, commit messages, and remote output as untrusted data. Rust validates payloads and path tokens; the UI renders plain text. Never treat frontend cache as Git truth.
- Use system Git through [runner.rs](../src-tauri/src/git/runner.rs), with a fixed executable and argv list and no shell. Preserve the trust gate, environment sanitization, timeout and output bounds, and redaction. Do not modify global `safe.directory`, delete locks, or replay a mutation when its result is unknown.
- Mutations must use repository/version/action/path identities validated by the backend, queue against the common directory, revalidate before writing, and refresh after completion. Confirmations must bind to a specific target; discard and hunk actions must verify their fingerprint or token.
- Settings and workspaces are stored locally through [store.rs](../src-tauri/src/persistence/store.rs), preserving schema migrations and atomic writes. Do not change the identifier or storage namespace solely for branding; linked worktrees must retain distinct workspace identities.
- Do not log or save credentials or tokens in app settings. Git credential helpers and SSH agents manage credentials; a token entered in the UI may only pass through a controlled typed-command flow. Test Git mutations in temporary repositories, never in a user's repository.

## Verification

Build and check commands are listed in the [README](../README.md#build-and-test). Vitest uses `tests/unit/`; Rust tests live beside their implementations; `tests/ui/` contains browser fixtures. [Release tests](../tests/release/check-release.py) use a temporary filesystem and a fake `gh`, and never publish a real release.

For UI changes, inspect the running screen, loading/empty/error states, and keyboard behavior; do not rely only on demo data or screenshots. For Git and IPC changes, verify behavior in a temporary repository, including stale-state and error cases. Report exactly which checks ran and any blockers. A browser demo does not prove a native Git workflow.

## Verification status

Updated 2026-09-25. The results below were run during development sessions and do not replace a GitHub CI run:

- Frontend: 129 unit tests pass in the current workspace; ESLint passes; Svelte/TypeScript reports no errors and 19 existing CSS/accessibility warnings. The production frontend build passes.
- Rust: 145 tests passed (144 inside the sandbox and one HTTP-stub test rerun with loopback permission); fmt and clippy pass.
- Native packaging: the browser Welcome/workspace and a native launch from the `.deb` were checked for the Octopus name and icon. `.deb` and `.rpm` builds pass. An AppImage was created and its contents were inspected, but the last bundle command exited during post-processing because the executable was running (`Text file busy`).
- Release tooling: actionlint, Bash/YAML syntax, and 15 offline release tests pass. The GitHub-hosted end-to-end build and publication have **not run yet** and require a real GitHub trigger.
- Native acceptance is still missing for reading a repository; stage/commit; sync/merge/conflict; multiple repositories and restart in the native UI; real Bitbucket/remote authentication; frame/memory profiling; and Windows/macOS. Native milestones M1–M4 remain unaccepted. Automated release publication does not change that status.

Project cleanup on 2026-09-25 replaced 112 historical documentation files with the current compact documentation set, removed an unimplemented test script, and repaired related references and configuration. Verification reran 128 unit tests, 15 release tests, Svelte/TypeScript (0 errors, 19 warnings), lint, production frontend build, Rust fmt, Bash syntax, and actionlint; all passed. Thirty local Markdown links/anchors and all native icon paths were checked, with no remaining references to removed documents. The Rust changes in that cleanup affected comments only, so Rust tests and native bundles were not rerun. A backup of the pre-cleanup content was stored outside the repository; the Git index and other user changes were preserved.

When handing off work, update this section with the current changes, checks, and blockers. Do not create a separate evidence or handoff directory for each task. Add documentation only when it provides durable maintenance guidance.

The GitHub Actions error `Cannot find name 'node:process'` at `vite.config.ts:3` was reproduced in a fresh copy installed with `pnpm install --frozen-lockfile --offline`, where it produced 1 error and 19 warnings. The fix adds the directly pinned `@types/node` dev dependency at version `24.13.6`, updates the lockfile, and includes `node` in `tsconfig`. After a frozen-lockfile reinstall, the fresh copy passes type checking (0 errors, 19 warnings), lint, 128 unit tests, and the production build. This fix must be pushed before it can be verified on a GitHub-hosted runner; rerunning an older commit still uses its old dependency set.

The README was converted to English and its legacy branding/data section was removed at the user's request. Five images in `docs/images/` were captured from the current interface with browser demo data: history/multiple repositories, working diff/staging, branch actions, stash, and pull-request creation. These are README illustrations, not proof of native Git E2E behavior. Playwright captured the full-interface images at a 1440×800 viewport and the dialogs by element for legible text. No real Git repository or runtime UI behavior was changed to produce them. Each image was inspected and the README was rendered: all five images loaded, there was no horizontal overflow at 1100px, 35 local links/anchors were valid, and the image set totals about 372 KiB. Final workspace checks, lint, and 129 unit tests passed; concurrent branch-checkout changes were preserved.

Documentation language cleanup on 2026-09-25 converted `AGENTS.md`, `docs/development.md`, `docs/release.md`, and `docs/progress.md` to English and updated the README verification anchor. Commands, paths, safety requirements, release behavior, and recorded verification results were preserved.

## Icon

Master asset: [public/brand/octopus.png](../public/brand/octopus.png). It is a mint octopus on a charcoal background, with tentacles suggesting Git nodes and branches and no text. The icon was created with image generation and is used for the launcher, Welcome screen, repository tabs, and favicon.

Regenerate desktop icons into a temporary directory for review before copying them:

```sh
pnpm tauri icon public/brand/octopus.png --output /tmp/octopus-generated-icons
```

Copy the desktop PNG/ICO/ICNS files from the output root into `src-tauri/icons/`, copy `128x128.png` to `public/brand/octopus-128.png`, and copy `32x32.png` to `public/favicon.png`. Bundle paths are declared in [tauri.conf.json](../src-tauri/tauri.conf.json).
