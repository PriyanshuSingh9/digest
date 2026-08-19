# agents.md

## Package manager: pnpm

- This project uses **pnpm**. Do not use `npm`, `yarn`, or `bun` for installing dependencies, running scripts, or managing lockfiles.
- `pnpm-lock.yaml` is authoritative and must be committed in sync with `package.json`. If a stale `package-lock.json` exists, ignore it — do not update or treat it as canonical.

## No emojis anywhere

- Never use emoji characters in the project: not in code, comments, documentation, UI copy, commit messages, or file names.
- Exception: when reproducing example raw article content verbatim, keep the original characters exactly as they appear in the source.

## Documentation update rule on commits

- Whenever the user asks to make a git commit, finish a task, or ship a feature, you **MUST update the documentation in the same change**.
- Update and tick completed checkboxes in [docs/milestones.md](docs/milestones.md).
- If any IPC command, struct, or interface changed, update [docs/contracts.md](docs/contracts.md) and both Rust/TypeScript types simultaneously.
- If any non-trivial architectural choice was made, record it in [docs/decisions.md](docs/decisions.md).
- Keep docs living and accurate — never defer documentation updates to a later commit.

## Project Structure & Ownership

- This is a single-developer project structured as a Tauri v2 native desktop application.
- **Rust Core (`src-tauri/`)** owns durable state, SQLite persistence, filesystem artifact storage, process supervision, ACP client communication, audio pipeline execution, and deterministic validation.
- **Web Frontend (`src/`)** owns presentation, interactive canvas rendering, synchronized playback UI, and user controls.
- **Shared Seam (`docs/contracts.md`)** defines the strongly-typed interfaces between Rust and TypeScript. Any change to a contract must be updated in `docs/contracts.md`, Rust structs (`src-tauri/src/contracts/`), and TypeScript types (`src/types/contracts/`) simultaneously.
