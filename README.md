# Digest

> Turn long-form technical articles into synchronized, narrated, interactive learning presentations.

Digest is a privacy-first, local-first native desktop application that transforms complex web articles into audio-synchronized, interactive learning experiences. Audio narration, highlighted text, code walkthroughs, diagrams, and visual demonstrations advance together on a deterministic timeline.

---

## Core Invariant

> **The agent plans and compiles; Rust owns durable storage, job execution, and orchestration; React owns presentation and deterministic playback.**

---

## How It Works

1. **Ingestion & Normalization** -- Ingest web article URLs or raw Markdown. Cleans HTML, preserves structural hierarchy (headings, paragraphs, code blocks, lists, callouts), and extracts canonical article metadata into SQLite.
2. **Concept & Content Analysis** -- Evaluates source text to identify core ideas, supporting concepts, causal flows, code snippets, quantitative claims, and high-value visualization opportunities.
3. **Narration Scripting** -- Generates conversational, technically accurate spoken narration scripts in configurable modes (Faithful, Explained, Deep Dive, Executive) with clear boundaries between source claims and supplementary explanations.
4. **Audio Synthesis & Pluggable TTS** -- Synthesizes speech via local TTS sidecars (e.g. Piper/Kokoro) or remote API providers with voice customization, speed control, and chunk caching.
5. **Timestamp Alignment** -- Generates paragraph, sentence, and word-level audio alignment timestamps to anchor all presentation events to the audio clock.
6. **Presentation Planning & Visual Generation** -- Maps narration segments to visual components: synchronized text highlighting, interactive code stepping, structured Mermaid/SVG diagrams, data charts, and animated state machines.
7. **Timeline Assembly & Storage** -- Persists the compiled presentation timeline, synchronized segments, and visual component states directly to SQLite and content-addressed storage.
8. **Deterministic Playback Runtime** -- Plays audio and drives 60 FPS UI transitions, text scrolling, and visual animations strictly through a single master playback clock with zero runtime LLM dependency.

---

## Technology Stack

| Layer | Technology | Notes |
| :--- | :--- | :--- |
| **Desktop Shell** | [Tauri v2](https://v2.tauri.app/) | ~20 MB installer, ~60 MB RAM footprint, capability-based security model |
| **Frontend** | React 19, TypeScript, Vite, TailwindCSS | 60 FPS synchronized playback canvas, custom media timeline controls |
| **Core Engine** | Rust (`tokio`, `rusqlite`, `reqwest`, `serde`) | Job state machine, stream processing, validation engine, storage manager |
| **Persistence** | SQLite in WAL mode | Single Database Writer Actor pattern, crash-recoverable job queues |
| **Storage** | Content-Addressed File Store | Immutable artifacts and cached audio chunks indexed by SHA-256 |
| **Intelligence Layer** | ACP (Agent Client Protocol) Bridge | Pluggable harness (Codex, Antigravity, Claude, custom agents) |
| **Audio Synthesis** | Pluggable TTS Provider | Local TTS sidecars (Piper, Kokoro) + Cloud TTS APIs |
| **Alignment** | Multi-tier Synchronizer | Word-level alignment fallback chain (Word -> Sentence -> Paragraph) |
| **Visual Components** | Structured Declarative Renderers | Semantic SVG, Canvas, Mermaid, Prism/Shiki syntax highlighting |

---

## Project Structure

```
digest/
|-- docs/
|   |-- README.md                # Documentation home and navigation map
|   |-- architecture.md          # Master system architecture & technical invariants
|   |-- milestones.md            # Master development roadmap (Features 1-15 across 5 sprints)
|   |-- contracts.md             # Shared typed contracts (TS <-> Rust & IPC models)
|   |-- decisions.md             # Architectural decision log (ADR format: D-001 ...)
|   `-- setup.md                 # Teaching guide for workspace setup & minimal dependencies
|-- src/                         # React 19 + TypeScript frontend application
|   |-- components/              # UI widgets (Player, Canvas, Library, Dropzone)
|   |-- features/                # Domain views (Presentation, Inspector, Settings)
|   |-- player/                  # Master playback clock & timeline synchronizer
|   |-- presentation/            # Declarative visual component renderers
|   |-- services/                # Tauri IPC bridge & mock data providers
|   `-- types/contracts/         # Hand-maintained TypeScript contract types
|-- src-tauri/                   # Rust core engine & Tauri v2 backend
|   |-- src/
|   |   |-- commands/            # Tauri IPC command handlers
|   |   |-- jobs/                # Durable generation job state machine
|   |   |-- storage/             # SQLite WAL connection & Database Writer Actor
|   |   |-- artifacts/           # Content-addressed filesystem artifact manager
|   |   |-- acp/                 # Agent Client Protocol client bridge
|   |   |-- audio/               # Pluggable TTS provider & alignment integration
|   |   |-- validator/           # Timeline and alignment validation engine
|   |   |-- contracts/           # Hand-maintained Rust contract structs & enums
|   |   |-- lib.rs               # Tauri command registration & plugin setup
|   |   `-- main.rs              # Application entry point & Tokio async runtime
|   |-- Cargo.toml               # Rust dependencies
|   `-- tauri.conf.json          # Tauri capability & window configuration
|-- public/                      # Static assets, fonts, icons
|-- package.json                 # Node dependencies and scripts
|-- tsconfig.json                # TypeScript configuration
`-- vite.config.ts               # Vite bundler configuration
```

---

## Getting Started

### Prerequisites

- **Node.js**: v20 or higher
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
# Run frontend only with Vite (browser dev mode with mock fixtures)
pnpm dev

# Run desktop application (Tauri + React + Rust backend)
pnpm tauri dev
```

### Testing

```bash
# Run Rust unit & integration tests
cargo test --manifest-path src-tauri/Cargo.toml

# Run TypeScript type check
pnpm build
```

---

## Core Invariants & Privacy

- **Audio Is the Timeline** -- The audio playback clock is the single source of truth for synchronization. Text highlights, code steps, and visual transitions are driven deterministically by this clock.
- **Agent Plans, Runtime Executes** -- The external agent acts as an offline compiler producing the presentation timeline and assets. Once generated, playback requires zero LLM calls and functions 100% offline.
- **Source Fidelity** -- Generated explanations, concept analogies, and supplementary notes are strictly separated from direct author claims in the data model and UI presentation.
- **Local-First & Private** -- Article texts, generated audio, intermediate analysis, and learning history are stored locally in SQLite and content-addressed filesystem storage.
