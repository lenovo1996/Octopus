# Instructions for AI agents working on Octopus

## CodeGraph

If a `.codegraph/` directory exists at the repository root, use CodeGraph before grep/find or reading source files to understand or locate an implementation. Use the `codegraph_explore` MCP tool when available, or `codegraph explore "<symbol names or question>"`. If `.codegraph/` does not exist, skip CodeGraph and do not create an index.

## Read before editing

1. Read `README.md` and `docs/development.md`; also read `docs/release.md` for build or release work.
2. For UI work, inspect the running interface and `src/lib/styles/`; use the icons in `public/` and `src-tauri/icons/`.
3. For backend work, read the typed contracts in `src/lib/ipc/types.ts`, the relevant Rust DTOs/commands, and the Git engine. Preserve the Git and data constraints in `docs/development.md`.
4. Confirm that task dependencies are complete, and edit only what the current task requires.

## Implementation constraints

- Use Rust, Tauri 2, Svelte 5, strict TypeScript, and Vite, with Linux first. Do not change the stack or add a server, cloud account, or AI runtime to the product without an explicit request.
- Use Svelte 5 runes for new state. The UI must not invoke a shell, retain credentials, or treat cached state as Git truth.
- Route every Git operation through a typed command and the Rust Git engine. Do not create a `run_command(string)` API or accept arbitrary argv from the frontend.
- Do not expand feature scope without a request. Do not turn unsupported functionality into fake buttons.
- Use temporary repositories for mutation tests. Never use a user's real repository as a test fixture.
- Do not run `reset --hard`, `clean`, force-push, delete files/repositories, or rewrite user history without specific confirmation. Do not commit, push, or publish on the user's behalf.
- Do not run a generator that overwrites the existing project to fix a scaffold problem.
- Do not spawn subagents automatically. Delegate only when the user asks; otherwise, one agent performs the task sequentially.

## Definition of done for each task

The code meets its acceptance criteria; relevant checks pass; the UI includes loading, empty, and error states where applicable; logs contain no secrets; and the current status, checks, and blockers are recorded in `docs/development.md`. Do not create separate evidence or handoff directories for each task. If the environment cannot run a check, mark that gate as `blocked` and state the exact reason. Never claim verification in place of actually running a check.

## Output style

The reader has ADHD. Write project documentation in English, respond to the user in their language unless requested otherwise, and keep technical identifiers in English.

1. Lead with the result or next action: command, path, or snippet first.
2. Number multi-step work, with one bounded action per step.
3. End with one action that can be completed in under two minutes.
4. Finish the current issue before raising a new one.
5. State progress on every turn, for example, “step 3/5 complete.”
6. Give estimates in concrete minutes, hours, or days, and label them as estimates.
7. After a change, state what now works and provide evidence.
8. For errors, give the location, cause, and fix.
9. Group and rank long lists, aiming for no more than five items per group.
10. Avoid preambles, recaps, and generic closing remarks. Explain fully when asked. Confirm destructive actions first. After three failed attempts at the same fix, stop and name the assumption that is now doubtful. If a request is ambiguous, ask one short question.
