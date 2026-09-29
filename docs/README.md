# Digest — Documentation Home

> The entry point for developing Digest. Read this first, then follow the links.
> The authoritative product and architecture document is [`article_learning_engine_prd.md`](../article_learning_engine_prd.md) at the repository root. Every document linked below describes the code as it exists at commit `3d99c32`; where the original plan was not built, the document says so and names the file that exists instead.

---

## Why this structure exists

Digest is an AI technical article learning and presentation engine that transforms long-form web articles into audio-synchronized, interactive learning lessons. It pairs a native Rust desktop core with a React frontend, drives an external agent harness over ACP (Agent Client Protocol), and exposes Digest's own deterministic tools to that agent over MCP.

These docs exist so that:

- **The build order is unambiguous** -- One canonical roadmap defining problem statements, key files to touch, requirements, alternatives, and verification steps.
- **Architectural boundaries stay sharp** -- The agent acts as an offline compiler; the Tauri application acts as a deterministic runtime player.
- **The IPC seam is typed** -- Hand-maintained contracts in Rust and TypeScript keep backend and frontend in lockstep without codegen overhead.
- **Decisions stay recorded** -- The decision log captures *why* choices were made in ADR format with deep trade-off analysis, preventing repetitive architectural debate.
- **Playback remains deterministic** -- The audio playback clock is the single source of truth; playback never depends on a live LLM.

---

## The Docs Map

| Document | Purpose | Audience | Status |
| :--- | :--- | :--- | :--- |
| [Master Roadmap](milestones.md) | What we build, why, which files to touch, and verification (Features 1-15 across 5 Sprints) | Canonical scope & build plan | Living |
| [System Architecture](architecture.md) | How the system is designed and why (glossary, topology, patterns, storage) | Complete system design | Living |
| [Shared Contracts](contracts.md) | The typed seam between Rust and React: Tauri commands, artifacts, MCP tools, and the playback manifest | System interfaces & IPC | Living |
| [Decision Log](decisions.md) | Architectural decisions recorded ADR-style (D-001 … D-015) with trade-off tables | Architectural history | Append-only |
| [Research: Agent, Ingestion, Runtime](research-agent-ingestion-runtime.md) | Pre-implementation research note on ACP, article capture, and Astro versus React. Not an accepted decision | Historical input | Snapshot, 2026-08-13 |

The PRD, [`agents.md`](../agents.md) (binding working rules), and [`HANDOFF.md`](../HANDOFF.md) (current implementation state) live at the repository root, not in this folder.

---

### Where specific knowledge lives

| You want to know... | Go to |
| :--- | :--- |
| What to build next, key files to touch, and verification steps | [milestones.md](milestones.md) |
| How the playback clock synchronizes text, seeking, and the transcript | [architecture.md](architecture.md#51-master-playback-clock--synchronization-model) |
| How the external agent communicates with Tauri over ACP, and reaches Digest's tools over MCP | [architecture.md](architecture.md#53-acp-client--agent-harness-bridge) and [contracts.md](contracts.md#the-mcp-tool-seam) |
| Who owns which typed IPC command or payload | [contracts.md](contracts.md) |
| What playback manifest `1.2` contains, and how `1.0`/`1.1` still play | [contracts.md](contracts.md#playback-manifest-schema-12) |
| Why we chose Tauri v2, SQLite WAL, per-sentence Ogg Opus, or declarative components | [decisions.md](decisions.md) |
| How to get the repository running | [`../run_demo.sh`](../run_demo.sh), or `pnpm install && pnpm tauri dev` |
| What the product is and who it is for | [`PRODUCT.md`](../PRODUCT.md) and [article_learning_engine_prd.md](../article_learning_engine_prd.md) |

---

## Conventions

1. **The PRD is authoritative for scope.** `article_learning_engine_prd.md` is the product, architecture, contract, and milestone authority. [milestones.md](milestones.md) is the build-order tracker against it.
2. **Contracts are a promise.** Any change to an IPC command or data structure must land in three places in the same change: [contracts.md](contracts.md), the Rust type in its owning module under `src-tauri/src/`, and the TypeScript type in `src/App.tsx`. There is no `src-tauri/src/contracts/` or `src/types/contracts/` directory.
3. **Decisions get logged.** Any architectural choice with more than one viable option goes into [decisions.md](decisions.md) in ADR format. Decisions are appended, never silently edited.
4. **No emojis anywhere.** As specified in [agents.md](../agents.md), never use emoji characters in code, comments, documentation, UI copy, commit messages, or file names.
5. **No caps on output.** Do not cap findings, narration segments, lesson duration, or model output. Rendering may use collapsed previews, but complete output must remain durable and inspectable.
6. **Tick the checkboxes.** Progress trackers live in [milestones.md](milestones.md). Keep them current as features land.
7. **Name real files.** Where a document names a path that does not exist, replace it with the real path and say so rather than leaving the plan in place.
8. **Small, focused diffs.** Documentation is living: update docs in the same change as the code they describe.

---

## Folder conventions

- `docs/*.md` -- Project-wide documentation (roadmap, architecture, contracts, decisions, setup).
- `docs/learn/` -- Learning material kept alongside the project, not part of the build.
- File names are lowercase-kebab (`contracts.md`, `milestones.md`, `architecture.md`) so internal links remain predictable and stable.
- The glossary lives in [architecture.md](architecture.md#glossary) -- one canonical glossary, referenced everywhere.

---

## How development progresses

1. Pick the next incomplete feature from [milestones.md](milestones.md).
2. Check the dependency graph in [milestones.md](milestones.md#dependency-graph) to confirm prerequisites are satisfied.
3. Check [contracts.md](contracts.md) for the types and commands the feature will touch.
4. There is no standalone browser mode: the UI calls real Tauri commands, so run it with `pnpm tauri dev`. `pnpm dev` serves the frontend but the commands will not resolve.
5. Implement the feature and verify all acceptance criteria listed in the feature breakdown. For Rust, `cd src-tauri && cargo test`; for the frontend, `pnpm build`.
6. Update [milestones.md](milestones.md), the PRD, contracts, and the decision log in the same commit.
