# Digest — Decision Log (Architectural Decision Records)

> Architectural decisions, recorded ADR-style with deep trade-off analysis so the *why* survives the *what*.
> New decisions **append**; existing ones get **superseded**, never silently edited.
> The tables in [architecture.md §3](architecture.md#3-technology-decisions) and [milestones.md — Waterfall Decisions](milestones.md#waterfall-decisions-made-once-inherited-by-everything) summarize the same choices.
> D-001 through D-010 were recorded at project kickoff. D-011 through D-015 were added as the audio pipeline and player landed through commit `3d99c32`.

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

## D-003 — Frontend stack: React 19 + TypeScript + Vite

- **Status:** Accepted
- **Date:** Project kickoff

**Context:** We need rapid UI development, robust component libraries for charts and syntax highlighting, and fast HMR inside the Tauri webview.

**Decision:** React 19 with TypeScript, and Vite as the bundler.

**Update (2026-09-05):** The TailwindCSS half of this decision was not adopted. There is no Tailwind dependency in `package.json` and no Tailwind build; all styling is hand-written CSS in `src/App.css`. The choice of React and Vite stands. See [D-015](decisions.md#d-015--progressive-manifest-normalization-into-a-single-clip-player) for the related decision to keep the player in one component file.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Svelte / SvelteKit** | Great performance, but smaller ecosystem for rich presentation widgets, AST-based code steppers, and syntax highlighting plugins. |
| **Vue 3** | Solid framework, but smaller community around interactive canvas and code walkthrough tooling. |
| **Vanilla TS / Web Components** | Too much boilerplate for managing complex reactive media player state, modals, and library filtering. |
| **TailwindCSS (planned, not adopted)** | The app is a single component file with a dark evaluation surface and a hand-written stylesheet. A utility-class layer would have had to be mixed with the existing CSS anyway, and none was needed. |

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

**Update (2026-09-05):** The clock remains authoritative, but it is now a per-clip clock rather than a whole-lesson clock. The player loads one sentence part at a time into a single audio element and derives the lesson position as the current clip's `startMs` plus the element's `currentTime`. The invariant is unchanged in substance — nothing advances on a timer — but the implementation is documented in [D-015](decisions.md#d-015--progressive-manifest-normalization-into-a-single-clip-player).

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

**Context:** The system handles two distinct categories of data: queryable structured state (article content, narration plans, audio references, job history) and binary media (raw HTML captures, audio, images).

**Decision:** SQLite in WAL mode for structured records, and content-addressed filesystem storage under `artifacts/objects/` for payloads and binary media.

**Update (2026-09-05):** Two parts of the original decision were not built.

- **No single writer actor.** `DigestService` opens a `rusqlite` connection per operation instead of serializing writes through a Tokio MPSC actor. WAL mode plus short-lived connections made the contention the actor was meant to remove rare enough that the actor was not worth the indirection.
- **No relative artifact paths.** The `artifacts` row stores `content_hash`, not a path. Objects live at `artifacts/objects/{sha256}.{json,bin}` and are joined by hash. The planned `articles/{article_id}/...` tree does not exist.

The decision to split structured records from binary media stands. See [D-012](decisions.md#d-012--durable-audio-per-sentence-ogg-opus-artifacts-over-a-concatenated-master-stream) for what the object store ended up holding.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Audio as BLOBs in SQLite** | Storing megabytes of audio binary in SQLite causes database file fragmentation, slow backups, and high memory spikes during read queries. |
| **Filesystem-only JSON files** | Relational queries (listing a job's artifacts, paging agent events, tracking attempt status) require loading and parsing hundreds of separate files. |
| **Single writer actor (planned, not built)** | Adds a channel and a serialized task for contention that WAL mode already avoids. Revisit only if concurrent writers produce `SQLITE_BUSY` in practice. |

**Tradeoffs & Caveats:**
- Two storage sinks must be kept consistent: the row and the object. Mitigation: the object is written and `fsync`ed before the row that references it is inserted, and content addressing makes a duplicate write a no-op rather than a conflict.
- There is no reference counting or garbage collection. Deleting a job's rows leaves its objects on disk.

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

**Decision:** A pluggable provider boundary, paired with a multi-tier alignment fallback chain (Word -> Sentence -> Paragraph/segment).

**Update (2026-09-05):** The provider boundary is built and holds; the alignment chain is not at parity.

- The boundary is the `AudioProvider` trait in `src-tauri/src/audio.rs`. Kokoro is the only implementation. Cloud providers were not added, and none are planned for V1.
- Tier 1 (word-level) is **blocked, not skipped**. Kokoros documents a timestamped ONNX model and TSV sidecars, but exposes them only through its CLI; its HTTP endpoint returns audio and no timings. Sentence-level alignment is delivered instead by transport subdivision, recorded in [D-013](decisions.md#d-013--sentence-boundary-transport-subdivision-for-generation-seeking-and-alignment).
- Sentence-level timings are derived by Digest from the container bytes, not read back from the provider. That constraint is its own decision: [D-011](decisions.md#d-011--durable-duration-from-container-bytes-not-provider-headers).

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Cloud-only TTS** | Fails offline, requires paid API keys, and sends article text to third-party servers. |
| **Local-only TTS** | Quality varies across hardware tiers, and high-quality local models require AVX2 / Apple Silicon / discrete GPUs. Kokoro was chosen as the one local provider V1 ships. |
| **Rigid Word-only Alignment** | Fails the entire generation job if the TTS engine lacks word timestamps. This is exactly the situation V1 is in, which is why the chain exists. |

**Tradeoffs & Caveats:**
- Sentence-level fallback provides less granular karaoke highlighting than word-level, but guarantees the lesson remains playable. Sentence granularity is the current floor, and the player resolves to a sentence part and no finer.
- The `AudioProvider` trait is deliberately narrow (name, MIME type, default voice, synthesize). Adding Piper or a cloud provider means implementing four methods, but it also means accepting the container-timing contract: a provider whose output Digest cannot independently time is rejected at generation time, not silently mis-timed.

**When to Revisit:**
- When browser-native Web Speech API or WebAssembly ONNX TTS achieves parity with native sidecars.
- When a provider exposes word timestamps over HTTP, which would unblock Tier 1.

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
- Requires discipline to update [contracts.md](contracts.md), the Rust struct in its owning module, and the TypeScript type in `src/App.tsx`.

**When to Revisit:**
- Revisit only if team size expands beyond a solo engineer.

**Update (2026-09-05):** The hand-authoring rule stands and the "three places" are now real: `contracts.md`, the Rust module that owns the type, and the type declarations at the top of `src/App.tsx`. The two directories this decision originally named, `src-tauri/src/contracts/` and `src/types/contracts/`, were never created.

---

## D-011 — Durable duration from container bytes, not provider headers

- **Status:** Accepted
- **Date:** 2026-09-05

**Context:** A playback manifest is only as trustworthy as its timings. Segment boundaries drive seeking, transcript highlighting, and the whole lesson clock, so a wrong duration is not cosmetic: it desynchronizes every downstream consumer, and the error is invisible until someone scrubs to the end and finds silence.

Providers report durations in headers, and those headers are unreliable in practice. Kokoro's WAV endpoint writes `0xFFFFFFFF` for unknown RIFF and data chunk lengths, which is legal-looking and wrong. Commit `7c3ee11` existed to cope with that single provider's quirk, which is exactly the fragility to avoid: a correctness fix for one provider's container quirk belongs in Digest, not in a growing list of provider special cases.

**Decision:** Digest derives every duration from the durable bytes it is about to store, using `durable_audio_duration_ms(mime_type, bytes)`. The manifest never records a provider-reported number. A format Digest cannot time is rejected before anything is persisted, via `AudioError::UnsupportedFormat`.

Ogg Opus durations follow RFC 7845: the final page's granule position minus the `OpusHead` pre-skip, counted in 48 kHz samples regardless of the encoder's input sample rate. The reader requires a single logical stream, an explicit end-of-stream page, and no data after it, so a truncated download cannot silently shorten a lesson. WAV durations come from the `fmt ` byte rate against the `data` chunk, treating a declared length of `0xFFFFFFFF` as "to end of file".

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Trust the provider's duration header** | One provider's malformed header becomes a corrupted manifest. The Kokoro `0xFFFFFFFF` case was the proof, not a hypothesis. |
| **Read the duration back with a decoder library (symphonia, ffmpeg)** | A dependency, a decode pass, and still a provider-shaped answer. Two lines of arithmetic over the container header are auditable and dependency-free. |
| **Keep provider headers, add a validation pass** | Validation can only catch a mismatch Digest can already detect. Deriving correctly removes the class of bug instead of sampling it. |
| **Ship an ffmpeg sidecar** | An external process in the audio path for arithmetic that RFC 7845 already specifies. |

**Tradeoffs & Caveats:**
- Each new durable format requires a new timing function before that format can be used. That is the intended cost: the boundary is what makes the manifest trustworthy.
- The granule method requires a complete file. Streaming a response whose end-of-stream page has not arrived yields an error, not a guess.
- The method is exact for duration but says nothing about silence, leading silence trimmed by an encoder, or gaps between parts. Parts are contiguous by construction, so part boundaries are computed by accumulation and inherit any such artifact.

**When to Revisit:**
- Revisit if a provider is needed whose container cannot be timed from its own header. The answer would be a decoding dependency, not a header trust relaxation.

---

## D-012 — Durable audio: per-sentence Ogg Opus artifacts over a concatenated master stream

- **Status:** Accepted
- **Date:** 2026-09-05

**Context:** The original audio plan was to synthesize one file per narration segment and concatenate them into a single `master.opus` the player streams end to end. Evaluating a real 29-minute lesson showed two problems with that shape.

Storage: the segments were float32 WAV at 24 kHz, 158.9 MiB of raw audio in a 173 MiB data directory. A Chromium/WebView client also has to decode float32 WAV, which is not a container it is tuned for. Compression alone would have fixed both, but concatenation would not have.

Operability: a master stream has no addressable unit. Regenerating one sentence means re-encoding the whole lesson; seeking means a byte range; a single bad sentence means re-synthesizing everything. Segmentation was therefore worth keeping for its own sake, not just for file size.

**Decision:** Durable audio is **one Ogg Opus artifact per sentence part**, and the lesson is a **versioned playback manifest** that references those artifacts and carries the timing. There is no concatenated master stream, and the manifest is the lesson.

Kokoro is asked for `response_format: "opus"`, and the provider contract requires a MIME type. A provider that cannot deliver a container Digest can time is rejected, so "compressed" is a floor rather than an aspiration. The evaluated lesson was 158.9 MiB as float32 WAV; the Opus path is projected at roughly a tenth of that, but the projection has not been measured end to end on a full lesson.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Concatenated `master.opus`** | No addressable unit. One edit means re-encoding the lesson, and a partial failure discards everything synthesized so far. |
| **Keep WAV, concatenate later** | Solves neither problem. WAV is large and is the format the webview decodes least efficiently. |
| **One artifact per narration segment, no subdivision** | Segment-level regeneration, but a long segment is still a coarse seek and a coarse retry. This is what manifest `1.1` was; [D-013](decisions.md#d-013--sentence-boundary-transport-subdivision-for-generation-seeking-and-alignment) moved it to sentence granularity. |
| **Stream Opus over HTTP to a player endpoint** | Adds a transport, a range-request contract, and a serving process to save disk that a 29-minute lesson occupies at single-digit tens of MiB. |
| **M4A / AAC** | Broadly supported, but the encoder story is worse than what the provider already produces, and Ogg Opus was already available from the provider. |

**Tradeoffs & Caveats:**
- Many small files instead of one. In practice a lesson has tens to low hundreds of parts, which is not a problem for a content-addressed store.
- Gaps between parts would be audible. They are contiguous by construction, because each part's `startMs` is the previous part's `endMs`.
- The manifest is now a required runtime input, not a convenience export. Anything that can play the lesson needs the manifest and the object store together.
- Existing WAV-based runs remain readable. The MIME type is per part, not per lesson, and the cache key includes it, so a lesson is not invalidated by the format change.

**When to Revisit:**
- Revisit if lesson length grows far enough that hundreds of part fetches during a seek-heavy session dominate startup. The mitigation at that point would be range requests over a merged file, not a return to an unaddressable stream.

---

## D-013 — Sentence-boundary transport subdivision for generation, seeking, and alignment

- **Status:** Accepted
- **Date:** 2026-09-05

**Context:** [D-012](decisions.md#d-012--durable-audio-per-sentence-ogg-opus-artifacts-over-a-concatenated-master-stream) settled the durable format but left alignment. Word-level timing is Tier 1 of the alignment chain and is blocked: Kokoros documents a timestamped ONNX model with TSV sidecars, but only for its CLI, and its HTTP endpoint returns audio and no timings. Waiting on an upstream HTTP extension would have left the whole chain at segment granularity indefinitely.

The obvious alternative — build a real alignment service, force-align the segment text against the synthesized audio, and produce word timings — is a large addition for a granularity the player does not yet need, and it would be fitted to one provider.

**Decision:** Sentence-level alignment is delivered by **transport subdivision**, not by an aligner. `split_into_sentence_parts` splits a segment's `ttsText` at sentence boundaries and each part becomes its own synthesis request, its own artifact, and its own timing entry in the manifest's `parts[]` array. Because a part is exactly the audio that speaks it, the part's bounds *are* the sentence's alignment. No aligner runs, and no provider is asked for timings.

Sentence ends are `.`, `!`, or `?`, plus any immediately following closing quote or bracket, followed by whitespace. Fragments shorter than 40 characters merge forward and a short trailing fragment merges backward, so abbreviations such as `E.g.` never become a degenerate synthesis request.

This is explicitly a **transport rule, never a content limit.** Concatenating the parts reproduces the complete `ttsText`; nothing is dropped, shortened, or summarized to make subdivision convenient.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Force alignment (whisper, a Montreal aligner, a hosted aligner)** | A heavyweight dependency or a network dependency to recover timings that transport subdivision yields exactly, for the one granularity the player can render. Also wrong by construction for a segment whose text was itself normalized for pronunciation. |
| **Wait for provider word timestamps** | Unblocks on an upstream roadmap. The blocked case would then have no alignment at all, which is the condition the degradation chain exists to prevent. |
| **Split only for synthesis, keep segment-level timing** | Would have produced sentence artifacts with no sentence timing, so part-granular seeking and part-granular regeneration would not have been possible. The split is only worth doing if the timing comes with it. |
| **Split on a fixed character budget** | Deterministic but arbitrary. It would cut mid-clause and mid-abbreviation, producing unnatural prosody and sentence boundaries that do not match the punctuation a reader sees. |
| **Cap or shorten segments to keep them synthesizable in one request** | Rejected on principle. `agents.md` forbids capping findings, narration segments, lesson duration, or model output, and a length limit that silently rewrites the author's prose is a content limit wearing a transport costume. |

**Tradeoffs & Caveats:**
- Prosody resets at every sentence boundary. For teaching narration this is acceptable, occasionally even helpful, but it is a real change from one continuous utterance.
- Sentence detection is punctuation-based and will not respect all typographic cases; semicolons and dashes do not split, and an abbreviation ending in a period inside a sentence is handled by the length merge rather than by understanding.
- Subdivision increases provider request count by roughly the sentences-per-segment factor. Request count is not the cost it looks like, because parts are cached by content hash and a repeated sentence is synthesized once per pass.
- The player can resolve to a sentence and no finer. Word-level highlighting remains unbuilt, and this decision is why that is cheap to add later: word timings would attach to the part that already exists.

**When to Revisit:**
- Revisit if a provider exposes word timings over HTTP. Then Tier 1 becomes available and `parts[]` becomes the sentence tier underneath it rather than a replacement for it.

---

## D-014 — Newest-artifact-wins cache resolution

- **Status:** Accepted
- **Date:** 2026-09-05

**Context:** The audio cache is a lookup by input hash into a table of durable `audio_segment` artifacts, not a mutable current-value store. Nothing is ever overwritten: a regeneration persists a *new* artifact and the cache key it computes is identical to the original. So for any given key the table can hold several artifacts, and "which one is the cache" has to be answered by a rule.

The obvious rule is first-wins, meaning the oldest artifact for a key. That rule is wrong the moment `regenerate_segment_audio` exists. A user who regenerates a segment because the pronunciation was wrong would hear the original audio again on the next ordinary generation pass, with no way to tell that their request was silently dropped.

**Decision:** Resolution is **newest artifact wins**. `list_artifacts` orders by `rowid`, which is insertion order, and the generation loop walks that list in reverse and takes the first match for a cache key. Regeneration is then naturally sticky: the new artifact is the newest for its key, so later passes pick it up.

The alternative to a rule — overwriting the existing artifact row — was rejected. Artifacts are immutable and content-addressed; two artifacts can legitimately share a cache key and differ in bytes, and an audio segment referenced by an already-published manifest must keep resolving to the bytes it was published with.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **First-wins (oldest artifact for the key)** | Silently reverts a forced regeneration on the next pass. The user's request would appear to succeed and then not take effect. |
| **Overwrite or update the existing row** | Breaks immutability. A manifest already on disk references an artifact by ID, and a segment's bytes changing underneath a published lesson is exactly the drift the content-addressed store prevents. |
| **Delete the superseded artifact** | Same drift problem, plus it loses the ability to compare or recover the previous take. |
| **Order by `created_at_ms`** | Two artifacts persisted in the same millisecond would tie, and the tiebreak would be arbitrary. `rowid` is a monotonic insertion counter and cannot tie. This is why the change was to `ORDER BY rowid` and not to a timestamp comparison. |
| **A separate `current_audio` pointer table** | Correct, and a second source of truth that must agree with the artifact table on every write, for a lookup that an ordering already answers. |

**Tradeoffs & Caveats:**
- Regeneration leaves orphaned-but-referenced-by-nothing artifacts on disk, and repeated regeneration of the same segment grows the table. There is no garbage collector. At one artifact per sentence take this is not a practical problem yet.
- The rule depends on `rowid` being insertion order, which is a SQLite implementation detail rather than a declared one. It is stable for `WITHOUT ROWID`-less tables, which is what `artifacts` is, and `rowid` is never reused after a delete.
- "Newest" is per cache key, not per segment. Regenerating one segment does not disturb the audio of any other segment, including other segments that happen to share a sentence.

**When to Revisit:**
- Revisit if a garbage collector is added, at which point superseded-but-still-referenced artifacts must be distinguished from genuinely orphaned ones. That is also the point at which a reference count or a manifest-side index becomes cheaper than reasoning about insertion order.

---

## D-015 — Progressive manifest normalization into a single clip player

- **Status:** Accepted
- **Date:** 2026-09-05

**Context:** Playback manifests are immutable artifacts. Bumping the schema to `1.2` to add sentence parts therefore did not replace the `1.0` and `1.1` manifests already sitting in real data directories, and the `1.0` and `1.1` WAV-based lessons are the ones an operator is most likely to want to replay.

Two ways out. Either migrate stored manifests forward on read, rewriting history so every artifact has one shape. Or keep artifacts as they were and teach the reader every version. The first is incompatible with the immutability the whole artifact model rests on, and it silently changes a durable record to accommodate a new reader.

**Decision:** Keep the artifacts, and normalize at the reader. The player accepts `1.0`, `1.1`, and `1.2` and flattens all three into one `PlaybackClip[]` — one playable clip per sentence part, or one per segment for the legacy schemas — with `segmentIndex` preserved on every clip. Everything downstream of that list works in a single shape.

The legacy fields are therefore genuinely optional in the TypeScript type: `parts` is absent on a `1.0`/`1.1` manifest, and `mimeType` and `audioArtifactId` are absent at segment level on a `1.2` manifest. Code that reads the playback model must handle both, and a stored lesson keeps playing.

The player is a component inside `src/App.tsx` rather than a `src/player/` module, with no separate store and no Zustand. Playback state is React state derived from the audio element.

**Alternatives Considered:**

| Alternative | Why Not |
| :--- | :--- |
| **Migrate manifests forward on read** | Rewrites an immutable durable record on every read, and a failed migration corrupts a lesson. The artifact's whole value is that it is the record. |
| **Migrate manifests forward once, on upgrade** | Same immutability violation, applied in bulk and eagerly, and it still cannot cover a manifest written by an older binary after the upgrade. |
| **Branch the player per schema version** | Multiplies the code paths that actually need to be maintained, in exactly the component that has to be correct about timing. One normalized list is one code path. |
| **Drop support for `1.0`/`1.1` lessons** | Breaks replay of the lessons that were actually evaluated. The WAV-based evaluation run in the handoff is one of them. |
| **Version the player component too** | Two players to keep correct, and no way to fix a bug in the old one without shipping a new binary anyway. |

**Tradeoffs & Caveats:**
- The optional `parts` / `mimeType` / `audioArtifactId` fields push a version check to every reader. The normalization is deliberately the single place that does it, and it happens once per manifest change.
- A clip list is per-manifest, not per-lesson-family. Two manifests for the same article normalize to two lists, which is the correct behavior for two genuinely different takes.
- A `1.0`/`1.1` lesson resolves to segment granularity even if the player could do better, because that is all its manifest carries. Subdivision is not retrofitted into stored audio.
- The player being one component in one file does not scale. When a second surface needs playback, the clip model and the clock arithmetic are the things to extract, not the component.

**When to Revisit:**
- Revisit when a `1.3` arrives, and the question is then whether the flat clip list is still the right normalization or whether a nested one is needed. Revisit sooner if a second playback surface appears, at which point the clip model should move into a shared module.
