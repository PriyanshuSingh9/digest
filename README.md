# Digest

> Turn long-form technical articles into synchronized, narrated, interactive learning presentations.

Digest is a privacy-first, local-first native desktop application that transforms complex web articles into audio-synchronized, interactive learning experiences. Audio narration, highlighted text, code walkthroughs, diagrams, and visual demonstrations advance together on a deterministic timeline.

---

## Core Invariant

> **The agent plans and compiles; Rust owns durable storage, job execution, and orchestration; React owns presentation and deterministic playback.**

---

## How It Works

The stages below reflect the code at commit `3d99c32`. Stages marked *not built* are the remaining gap in the runtime path; the authoritative plan is [`article_learning_engine_prd.md`](article_learning_engine_prd.md) and the per-feature status is in [docs/milestones.md](docs/milestones.md).

1. **Ingestion & Normalization** -- Captures an article URL over bounded HTTP with manual redirect handling and rejection of private or loopback targets. Stores an immutable raw capture, then a normalized article of ordered blocks (heading, paragraph, code, list, quote, diagram), image metadata with resolved `srcset` candidates, and extraction diagnostics. Raw Markdown and HTML input are *not built*.
2. **Agent Analysis** -- An ACP-driven agent reads the normalized article through the `read_artifact` MCP tool and writes findings through `write_analysis`. Every finding carries a category, a `ProvenanceKind`, and the source blocks it came from.
3. **Narration Scripting** -- The agent calls `write_narration_plan` with segments that keep display text and TTS text separate and are labeled with intent, importance, and provenance. Every source block and every diagram must be taught, summarized, or skipped with a rationale, and TTS text that adds or drops content is rejected. The four narration modes in the PRD are not a selectable input; there is one compilation prompt.
4. **Audio Generation** -- Kokoro's local OpenAI-compatible speech endpoint returns Ogg Opus for each sentence. Every completed part is persisted and cached by input hash, so a retry, an edit to one sentence, or a restart re-synthesizes only what changed. Durations are derived by Digest from the container bytes, never from provider headers.
5. **Sentence Alignment** -- Narration segments are subdivided at sentence boundaries into transport parts, each with its own artifact and timing. This is the sentence-level alignment tier. Word-level timing is *not built*: Kokoros exposes timestamps only through its CLI, not its HTTP endpoint.
6. **Manifest Assembly** -- A versioned playback manifest records the audio configuration, the segments, and per-sentence parts with their timing, provenance, and presentation. The manifest is the timeline; there is no separate lesson table.
7. **Deterministic Playback** -- The player loads one sentence part at a time over raw Tauri IPC, derives the lesson position from the audio clock, and drives transcript auto-scroll, part-granular seeking, and playback speed from it. Manifests from earlier schema versions normalize into the same clip model and still play. The structured visual renderers -- diagrams, code walkthroughs, charts -- are *not built*; presentation data reaches the manifest as opaque JSON.

---

## Technology Stack

| Layer | Technology | Notes |
| :--- | :--- | :--- |
| **Desktop Shell** | [Tauri v2](https://v2.tauri.app/) | Small binary, low memory footprint, capability-based security model |
| **Frontend** | React 19, TypeScript, Vite | Single-file app (`src/App.tsx`) with hand-written CSS in `src/App.css`. No Tailwind, no state library, no component directory |
| **Core Engine** | Rust (`tokio`, `rusqlite`, `reqwest`, `serde`, `rmcp`, `schemars`) | Flat modules, subprocess supervision, audio pipeline, MCP server |
| **Persistence** | SQLite in WAL mode | Three tables: `artifacts`, `agent_events`, `run_attempts`. No writer actor, no migration table |
| **Storage** | Content-Addressed File Store | `artifacts/objects/{sha256}.{json,bin}`, written to a temp file and atomically renamed. Audio parts cached by input hash |
| **Intelligence Layer** | ACP (Agent Client Protocol) over stdio | OpenCode natively; `agy` through an explicit adapter command |
| **Tool Layer** | MCP over stdio | The agent calls `ingest_article`, `read_artifact`, `write_analysis`, `write_narration_plan` |
| **Audio Synthesis** | `AudioProvider` trait; Kokoro implementation | Local, free, offline. Delivers Ogg Opus; a provider whose container Digest cannot time is rejected |
| **Alignment** | Sentence granularity via transport parts | Word-level is blocked on provider timestamps |
| **Visual Components** | Structured declarative JSON | Travels in the manifest; renderers not built |

---

## Project Structure

```
digest/
|-- article_learning_engine_prd.md  # Authoritative product, architecture, and phase document
|-- agents.md                       # Binding working rules (pnpm, no caps, no emojis)
|-- HANDOFF.md                      # Current implementation state and open work
|-- PRODUCT.md                      # Product design context
|-- DESIGN.md                       # Visual design tokens and rules
|-- docs/
|   |-- README.md                   # Documentation home and navigation map
|   |-- milestones.md               # Master roadmap (Features 1-15) and progress tracker
|   |-- architecture.md             # System architecture, glossary, patterns, storage
|   |-- contracts.md                # Shared typed contracts (Tauri commands, artifacts, manifest 1.2)
|   |-- decisions.md                # Decision log (ADR format: D-001 ...)
|   `-- research-agent-ingestion-runtime.md  # Pre-implementation research note
|-- scripts/
|   `-- run_demo.sh                 # Starts Kokoro + the Tauri dev app, cleans up on exit
|-- src/
|   |-- App.tsx                     # Whole frontend: run setup, activity, artifacts, player
|   |-- App.css                     # Whole stylesheet
|   `-- main.tsx                    # React entry point
|-- src-tauri/
|   |-- src/
|   |   |-- lib.rs                  # Module wiring and the tauri::generate_handler! list
|   |   |-- main.rs                 # Application entry point
|   |   |-- host.rs                 # Every #[tauri::command] handler and HostState
|   |   |-- application.rs          # DigestService: SQLite schema, artifacts, events, attempts
|   |   |-- ingestion.rs            # HTTP capture, normalization, image localization
|   |   |-- acp.rs                  # ACP client, supervision, cancellation, event projection
|   |   |-- tools.rs                # Validated MCP tool contracts
|   |   |-- mcp.rs                  # MCP server bound to stdio
|   |   |-- audio.rs                # AudioProvider, cache, duration derivation, manifest 1.2
|   |   `-- bin/digest-mcp.rs       # The MCP binary the agent launches
|   |-- tests/                      # Integration tests and fixtures
|   |-- Cargo.toml
|   `-- tauri.conf.json
|-- package.json                    # Node dependencies and scripts
|-- pnpm-lock.yaml                  # Authoritative lockfile
`-- vite.config.ts
```

---

## Getting Started

### Prerequisites

- **Node.js**: v20.19 or higher (Vite 7 requirement)
- **pnpm**: v9 or higher (`npm install -g pnpm`)
- **Rust**: stable toolchain (`rustup default stable`)
- **Platform Dependencies**:
  - **Linux**: `libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **macOS**: Xcode Command Line Tools
  - **Windows**: Microsoft C++ Build Tools & WebView2

### Installation

```bash
# Install frontend dependencies (pnpm only)
pnpm install
```

### Development

```bash
# Run the desktop application. This is the only mode that works: the UI calls
# real Tauri commands, so there is no mock/browser-only mode.
pnpm tauri dev
```

### Testing

```bash
# Run Rust unit and integration tests (54 passing, 1 ignored at 3d99c32).
# The ignored test hits a live Kokoro container.
cargo test --manifest-path src-tauri/Cargo.toml

# Type check and production build the frontend
pnpm build
```

---

## Core Invariants & Privacy

- **Audio Is the Timeline** -- The audio playback clock is the single source of truth. Nothing advances on a timer. The player loads one sentence part at a time and derives the lesson position, transcript highlight, and active segment from that clock, so text and audio cannot drift.
- **Durations Are Derived, Not Reported** -- Every segment and part duration is computed by Digest from the durable container bytes (RFC 7845 granule positions for Ogg Opus). A provider-reported number is never trusted, and a format Digest cannot time is rejected before anything is written.
- **Agent Plans, Runtime Executes** -- The external agent acts as an offline compiler producing the analysis, narration plan, and assets. Once generated, playback requires zero LLM calls and works entirely offline.
- **Source Fidelity** -- Generated explanations, inferences, and teaching scaffolding are tagged with a `ProvenanceKind` and strictly separated from source-derived claims in the data model and the UI. Every source block is explicitly taught, summarized, or skipped with a rationale.
- **No Caps On Output** -- Findings, narration segments, and lesson duration are not capped. Rendering may use collapsed previews; complete output stays durable and inspectable.
- **Local-First & Private** -- Article texts, generated audio, analysis, and run history are stored locally in SQLite and a content-addressed filesystem store under the OS app-data directory (`$HOME/.local/share/com.bhondu.digest` on Linux). Nothing leaves the machine except the article fetch and the local TTS request.

---

## Documentation

`article_learning_engine_prd.md` is the authoritative product and architecture document. `agents.md` holds binding working rules. For everything else start at [`docs/README.md`](docs/README.md): the [roadmap](docs/milestones.md), [architecture](docs/architecture.md), [contracts](docs/contracts.md), [decision log](docs/decisions.md). [`HANDOFF.md`](HANDOFF.md) records the current implementation state and what remains open.

## Running it

```bash
./run_demo.sh
```

That script starts the Kokoro TTS container, waits for it to answer, launches `pnpm tauri dev`, and on `Ctrl-C` tears down every process it spawned and stops the container. Use `./run_demo.sh --help` for the options.
