# agents.md

## Package manager: pnpm

- This project uses **pnpm**. Do not use `npm`, `yarn`, or `bun` for installing dependencies, running scripts, or managing lockfiles.
- `pnpm-lock.yaml` is authoritative and must be committed in sync with `package.json`. If a stale `package-lock.json` exists, ignore it — do not update or treat it as canonical.

## No emojis anywhere

- Never use emoji characters in the project: not in code, comments, documentation, UI copy, commit messages, or file names.
- Exception: when reproducing example raw article content verbatim, keep the original characters exactly as they appear in the source.

## Documentation authority

- [article_learning_engine_prd.md](article_learning_engine_prd.md) is the authoritative product, architecture, contract, milestone, and decision document.
- Other existing documents are historical unless the PRD explicitly promotes them.
- Whenever a commit changes product behavior, a public contract, a milestone, or a non-trivial architectural decision, update the PRD in the same change.
- Keep Rust and TypeScript representations of IPC contracts synchronized.

## Project Structure & Ownership

- This is a single-developer project structured as a Tauri v2 native desktop application.
- **Rust Core (`src-tauri/`)** owns durable state, SQLite persistence, filesystem artifact storage, process supervision, ACP client communication, audio pipeline execution, and deterministic validation.
- **Web Frontend (`src/`)** owns presentation, interactive canvas rendering, synchronized playback UI, and user controls.
- **Shared Seam** contracts are represented in Rust and TypeScript and governed by the architecture in `article_learning_engine_prd.md`.
