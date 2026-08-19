# Digest — Documentation Home

> The entry point for developing Digest. Read this first, then follow the links.
> Every document below lives in this `docs/` folder unless stated otherwise.

---

## Why this structure exists

Digest is an AI technical article learning and presentation engine that transforms long-form web articles into audio-synchronized, interactive learning lessons. It pairs a native Rust desktop core with a React frontend and connects to an external agent harness over ACP (Agent Client Protocol).

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
| [Master Roadmap](milestones.md) | What we build, why, which files to touch, and verification (Features 1–15 across 5 Sprints) | Canonical scope & build plan | Living |
| [System Architecture](architecture.md) | How the system is designed and why (glossary, topology, patterns, storage) | Complete system design | Living |
| [Shared Contracts](contracts.md) | The typed seam between Rust and React — IPC, data models, and ACP interfaces | System interfaces & IPC | Living |
| [Decision Log](decisions.md) | Architectural decisions recorded ADR-style (D-001 … D-010) with trade-off tables | Architectural history | Append-only |
| [Setup Guide](setup.md) | Step-by-step workspace setup, minimal dependencies, and verification | Developer onboarding | Living |

---

### Where specific knowledge lives

| You want to know... | Go to |
| :--- | :--- |
| What to build next, key files to touch, and verification steps | [milestones.md](milestones.md) |
| How the playback clock synchronizes text, code, and diagrams | [architecture.md](architecture.md#51-master-playback-clock--synchronization-model) |
| How the external agent communicates with Tauri via ACP | [architecture.md](architecture.md#53-acp-client--agent-harness-bridge) and [contracts.md](contracts.md#acpts--agent-client-protocol-acp) |
| Who owns which typed IPC command or payload | [contracts.md](contracts.md) |
| Why we chose Tauri v2, SQLite WAL, or declarative components | [decisions.md](decisions.md) |
| How to get the repository running from scratch | [setup.md](setup.md) |

---

## Conventions

1. **The master roadmap is canonical for scope.** Every feature is tracked in [milestones.md](milestones.md). If a feature's scope or technical approach changes, update the roadmap first.
2. **Contracts are a promise.** Any change to an IPC command or data structure must land in three places in the same change: [contracts.md](contracts.md), the Rust structs under `src-tauri/src/contracts/`, and the TypeScript interfaces under `src/types/contracts/`.
3. **Decisions get logged.** Any architectural choice with more than one viable option goes into [decisions.md](decisions.md) in ADR format.
4. **No emojis anywhere.** As specified in [agents.md](../agents.md), never use emoji characters in code, comments, documentation, UI copy, commit messages, or file names.
5. **Tick the checkboxes.** Progress trackers live in [milestones.md](milestones.md). Keep them current as features land.
6. **Small, focused diffs.** Documentation is living: update docs in the same change as the code they describe.

---

## Folder conventions

- `docs/*.md` -- Project-wide documentation (roadmap, architecture, contracts, decisions, setup).
- File names are lowercase-kebab (`contracts.md`, `setup.md`, `milestones.md`) so internal links remain predictable and stable.
- The glossary lives in [architecture.md](architecture.md#glossary) -- one canonical glossary, referenced everywhere.

---

## How development progresses

1. Pick the next incomplete feature from [milestones.md](milestones.md).
2. Check the dependency graph in [milestones.md](milestones.md#dependency-graph) to confirm prerequisites are satisfied.
3. Check [contracts.md](contracts.md) for the typed structs and interfaces required for the feature.
4. If building frontend components before backend commands exist, use mock fixtures in `src/mocks/` to test in standalone browser mode (`pnpm dev`).
5. Implement the feature and verify all acceptance criteria listed in the feature breakdown.
6. Update [milestones.md](milestones.md), contracts, and decision logs in the same commit.
