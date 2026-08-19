# Digest — Decision Log (Architectural Decision Records)

> Architectural decisions, recorded ADR-style with deep trade-off analysis so the *why* survives the *what*.
> New decisions **append**; existing ones get **superseded**, never silently edited.
> The tables in [architecture.md §3](architecture.md#3-technology-decisions) and [milestones.md — Waterfall Decisions](milestones.md#waterfall-decisions-made-once-inherited-by-everything) summarize the same choices.

---

## How to record a new decision

If a choice has more than one viable option, it is a decision -- record it here. A good rule of thumb: **if you explain "why we use X" more than once, it belongs in this log.**

Template:

```markdown
## D-XXX — Title

- **Status:** Accepted · Superseded by D-YYY · Rejected
- **Date:** YYYY-MM-DD

**Context:** what problem forced the choice?

**Decision:** what did we choose?

**Alternatives Considered:** (table with Alternative and Why Not)

**Tradeoffs & Caveats:** what are the costs and potential failure modes?

**When to Revisit:** what triggers an architectural review of this choice?
```

---

## D-001 — Desktop framework: Tauri v2

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** We are shipping a native desktop application that manages local article stores, audio playback, and subprocess execution. Low memory footprint, fast startup, and native IPC security are critical.

**Decision:** Use Tauri v2 as the native desktop shell.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Electron** | Electron bundles Chromium and Node.js with every app, resulting in a 150 MB+ binary and 150–200 MB baseline RAM footprint. It also has a weaker capability-based security model for protecting local files. |
| **Web-only SPA** | Cannot supervise local TTS sidecar processes, manage local SQLite files, or run offline filesystem pipelines securely without a separate backend daemon. |
| **Flutter / Qt** | Weaker web presentation ecosystem for rich interactive components (Shiki syntax highlighting, Mermaid rendering, HTML/DOM text layouts). |

**Tradeoffs & Caveats:**
- Uses the operating system's native webview (WebKitGTK on Linux, WebKit on macOS, WebView2 on Windows), which can have minor rendering differences across platforms.
- Linux requires platform dependencies (`webkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`).

**When to Revisit:**
- Revisit only if WebKitGTK platform differences on Linux cause severe rendering bugs that cannot be resolved via standard web standards.

---

## D-002 — Core engine: Rust with Tokio async runtime

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** The core backend must orchestrate long-running multi-stage generation jobs, manage SQLite transactions, supervise external TTS processes, stream audio files, and communicate over ACP without locking the UI.

**Decision:** Build the backend in Rust using the Tokio async runtime.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Node.js Sidecar** | Adds a heavy V8 runtime dependency, higher idle memory usage (~40 MB+), and weaker OS-level child process supervision compared to native Rust. |
| **Go** | Strong concurrency, but lacks Rust's zero-cost memory safety, native C-library interop for audio encoding without CGo overhead, and rich serde ecosystem. |
| **Python** | Global Interpreter Lock (GIL) limits true parallelism, heavy distribution overhead (bundling Python runtime on macOS/Windows), and slow string/DOM processing. |

**Tradeoffs & Caveats:**
- Slower compile times compared to Go/Node.js.
- Strict borrow checker requires thoughtful data structures (e.g. actor message passing over shared mutable state).

**When to Revisit:**
- Rust is foundational to Tauri v2 and will not be revisited.

---

## D-003 — Frontend stack: React 19 + TypeScript + Vite + TailwindCSS

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** We need rapid UI development, robust component libraries for charts and syntax highlighting, and fast HMR inside the Tauri webview.

**Decision:** React 19 with TypeScript, Vite bundler, and TailwindCSS v4.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Svelte / SvelteKit** | Great performance, but smaller ecosystem for rich presentation widgets, AST-based code steppers, and syntax highlighting plugins. |
| **Vue 3** | Solid framework, but smaller community around interactive canvas and code walkthrough tooling. |
| **Vanilla TS / Web Components** | Too much boilerplate for managing complex reactive media player state, modals, and library filtering. |

**Tradeoffs & Caveats:**
- React reconciliation overhead if components re-render carelessly during 60 FPS audio playback. Mitigation: keep playback clock subscriptions isolated in leaf components or Canvas loops.

**When to Revisit:**
- Revisit only if React reconciliation causes frame drops during rapid audio scrubbing on low-end hardware.

---

## D-004 — Architecture: Agent compiles timeline, runtime executes playback

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** Technical articles require deep AI reasoning (concept extraction, narration scripting, diagram generation). However, requiring live LLM inference during playback introduces latency, network fragility, nondeterministic rendering, and high cloud costs.

**Decision:** The external agent acts as an offline compiler producing structured timeline records in SQLite. The Tauri runtime deterministically executes the lesson with zero LLM calls during playback.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Real-time Streaming LLM Playback** | Causes variable pauses while the model generates text, seeking is impossible (cannot seek into ungenerated content), high token costs on every replay, and complete failure when offline. |
| **Pre-rendered Video (MP4)** | Takes minutes to render via ffmpeg, uses 500 MB+ per article, blurry text on high-DPI screens, uncopyable code, and inflexible theming. |

**Tradeoffs & Caveats:**
- Generation time takes 30–90 seconds up-front before the user can begin listening. Mitigation: show real-time progress inspector with stage breakdown.

**When to Revisit:**
- Revisit only if local LLMs become so fast (< 10ms token latency) that real-time conversational Q&A during playback becomes practical.

---

## D-005 — Synchronization clock: Audio timeline as master playback clock

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** Audio narration, text scrolling, word-level highlights, code step annotations, and visual animations must advance in lockstep without drift.

**Decision:** The HTML5 audio element's `currentTime` is the single authoritative clock. All visual and text states are pure functions of `currentTime`.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Independent JavaScript Timers (`setInterval`)** | Timers drift over multi-minute playback due to JS event loop lag and CPU throttling. |
| **Animation Frame Counter** | Desynchronizes whenever audio buffers, pauses, or changes playback speed. |

**Tradeoffs & Caveats:**
- The audio player must accurately report `currentTime` events. High-frequency updates should be driven via `requestAnimationFrame` reading `audio.currentTime` directly rather than depending solely on the browser's `timeupdate` event (which fires only 4 times/sec).

**When to Revisit:**
- This is a permanent mathematical invariant of the system.

---

## D-006 — Storage model: SQLite WAL for metadata/jobs + Local filesystem for artifacts

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** The system handles two distinct categories of data: queryable structured state (articles, sections, lessons, timeline segments, jobs, playback history) and binary media (raw HTML, audio WAV/Opus files).

**Decision:** SQLite in WAL mode with a Single Writer Actor for structured records and timeline segments; content-addressed filesystem storage under `artifacts/` for binary audio files and source dumps.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Audio as BLOBs in SQLite** | Storing megabytes of audio binary in SQLite causes database file fragmentation, slow backups, and high memory spikes during read queries. |
| **Filesystem-only JSON files** | Relational queries (filtering articles by tag, searching text, tracking job statuses) require loading and parsing hundreds of separate files. |

**Tradeoffs & Caveats:**
- Requires managing two storage sinks (database rows and filesystem paths). Mitigation: SQLite stores relative artifact paths, and article deletion cascades to delete corresponding files on disk.

**When to Revisit:**
- Revisit if article library exceeds 10,000 items and requires full-text search optimization via SQLite FTS5.

---

## D-007 — Agent integration: ACP (Agent Client Protocol) abstraction

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** Digest needs an intelligence layer to analyze articles, write narration, and plan diagrams. Coupling the application to a single LLM vendor (e.g. only OpenAI or only Anthropic) creates vendor lock-in and limits local execution options.

**Decision:** Implement an ACP (Agent Client Protocol) client abstraction in Rust communicating via stdio or WebSocket.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Hardcoded Vendor API Client** | Locks the codebase to specific model endpoints (e.g. OpenAI only) and prevents users from using local models or custom developer agents. |
| **Embedded Python Interpreter** | Heavy runtime distribution overhead (bundling Python and virtual environments on Windows/macOS). |

**Tradeoffs & Caveats:**
- Requires the user or environment to provide an ACP-compatible agent harness (Codex, Antigravity, or custom local agent server).

**When to Revisit:**
- If a built-in lightweight local LLM sidecar (e.g. `llama-server`) is bundled in future versions, wrap it behind the same `AgentProvider` trait.

---

## D-008 — Audio generation: Pluggable TTS with multi-tier alignment fallback

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** Users need flexibility: zero-cost private offline TTS on low-end laptops, or high-fidelity cloud voices. Furthermore, not all TTS engines provide word-level timestamps.

**Decision:** Pluggable TTS architecture supporting local sidecars (Piper/Kokoro) and cloud APIs (OpenAI/ElevenLabs), paired with a multi-tier alignment fallback chain (Word -> Sentence -> Paragraph).

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Cloud-only TTS** | Fails offline, requires paid API keys, and sends article text to third-party servers. |
| **Local-only TTS** | Quality varies across hardware tiers, and high-quality local models require AVX2 / Apple Silicon / discrete GPUs. |
| **Rigid Word-only Alignment** | Fails the entire generation job if the TTS engine lacks word timestamps. |

**Tradeoffs & Caveats:**
- Sentence-level fallback provides slightly less granular karaoke highlighting than word-level, but guarantees the lesson remains playable.

**When to Revisit:**
- When browser-native Web Speech API or WebAssembly ONNX TTS achieves parity with native sidecars.

---

## D-009 — Visual strategy: Structured declarative components over video

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** Visual explanations (diagrams, code steps, charts) can either be rendered as pre-baked MP4 video files or rendered dynamically as web components.

**Decision:** Render declarative structured components (Mermaid, SVG, HTML/CSS Canvas, Shiki code) driven by keyframes against the audio clock.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Pre-rendered MP4 Video** | Takes minutes to render via ffmpeg, occupies 500 MB+ per article, blurry text on high-DPI screens, unselectable code text, and poor seeking performance. |
| **Static Screenshot Images (PNG)** | High storage footprint, fixed aspect ratios, and no interactive hover or theme adaptation. |

**Tradeoffs & Caveats:**
- Requires writing and maintaining dedicated React renderers for each component type (`DiagramRenderer`, `CodeWalkthroughRenderer`, `ChartRenderer`).

**When to Revisit:**
- This design provides superior performance and will remain standard.

---

## D-010 — Contracts: Manually authored types

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** The Rust core and React frontend share every data shape across IPC.

**Decision:** Hand-author shared types on both sides with [contracts.md](contracts.md) as the written source of truth. No code generation tooling.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Protobuf / OpenAPI Codegen** | Tooling complexity, build friction, and opaque compilation steps outweigh manual synchronization costs for a single developer. |

**Tradeoffs & Caveats:**
- Requires discipline to update [contracts.md](contracts.md), Rust structs, and TypeScript interfaces in the same commit.

**When to Revisit:**
- Revisit only if team size expands beyond a solo engineer.
