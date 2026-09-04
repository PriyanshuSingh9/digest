# PRD — Article Learning & Presentation Engine

**Version:** V1.1
**Status:** Implementation-ready
**Product:** Article Learning & Presentation Engine
**North Star:** **Paste an article. Press play. Learn.**

---

# 1. Product Definition

The Article Learning & Presentation Engine converts technical articles into synchronized, narrated, interactive lessons.

The system accepts an article URL and produces a versioned multimedia presentation consisting of:

- the original source material,
- structured article understanding,
- educational narration,
- generated audio,
- synchronized timestamps,
- structured visual/presentation components,
- provenance information,
- and a deterministic playback manifest.

The system is **agent-driven**.

The active workflow agent is the **authoritative semantic orchestrator**. In V1 this is OpenCode or `agy`, controlled through ACP. It determines what work needs to happen, which Digest MCP capabilities to invoke, in what order, and with what inputs.

The application provides deterministic execution capabilities through a structured tool interface.

The core architecture therefore separates:

```text
Agent
  ↓
Decides WHAT should happen
  ↓
Tool Interface
  ↓
Rust Core
  ↓
Determines HOW it happens reliably
  ↓
Artifacts
  ↓
Manifest
  ↓
Web Runtime
  ↓
Deterministic playback
```

The application must not secretly reconstruct or independently own the semantic compilation workflow.

---

## 1.1 Document Authority and Release Strategy

This PRD is the authoritative product and architecture specification for implementation. Other repository documents are historical unless this PRD explicitly incorporates them.

The product has two deliberate release stages:

```text
V1 — Local quality evaluation
  Tauri client on Linux
  OpenCode and agy as workflow agents
  Local Rust execution and storage

V2 — Remote autonomous operation
  Hermes Agent as the control-plane actor
  Remote Linux deployment
  The same Rust services, MCP tools, artifacts, manifests, and web runtime
```

V1 exists to determine whether the system can consistently produce high-quality lessons. V2 changes the actor and deployment topology; it must not require a rewrite of the compiler or playback model.

---

# 2. North Star

> **Paste an article. Press play. Learn.**

The system should make a long technical article feel like a carefully produced educational video without requiring the user to manually:

- read everything,
- research every concept,
- find diagrams,
- look up code,
- create notes,
- search for explanations,
- or produce their own audio.

The original PRD establishes the central source → narration → presentation → timeline → lesson model.

V1 preserves this model while changing one important architectural assumption:

**The agent, not the application, owns workflow orchestration.**

---

# 3. Goals

## 3.1 Primary Goals

V1 must:

1. Accept one or more article URLs.
2. Capture article source, extract readable structure, and localize relevant media.
3. Produce a canonical normalized article representation.
4. Preserve the original source faithfully.
5. Allow OpenCode or `agy` to analyze the article through a V1 ACP session.
6. Allow the agent to orchestrate the complete compilation workflow.
7. Generate educational narration.
8. Generate audio using local or cloud TTS.
9. Align narration with the audio timeline.
10. Generate a versioned presentation manifest.
11. Play the presentation deterministically in a browser.
12. Preserve provenance from generated material back to source material.
13. Persist intermediate artifacts.
14. Support retries and partial regeneration.
15. Work locally first.
16. Allow the generated lesson to be accessed from another device through a local HTTP server and Cloudflare Tunnel.
17. Keep the core compiler independent of any specific desktop shell or agent harness.
18. Evaluate output quality using a fixed benchmark corpus and explicit quality criteria.

---

# 4. Non-Goals for V1

V1 will not attempt to build:

- a general-purpose autonomous research agent,
- continuous web research,
- a full personal knowledge graph,
- automatic fact-checking against the entire internet,
- advanced generated animations as a requirement,
- real-time LLM-driven playback,
- a mobile native application,
- a hosted multi-user SaaS platform,
- a single permanently embedded AI model,
- Hermes Agent integration,
- Codex, Claude Code, Grok Build, or other agent-provider integrations,
- a native mobile application,
- a native Rust presentation UI,
- macOS-specific packaging or browser compatibility,
- Firefox-specific compatibility,
- dependence on one agent harness.

Future inputs such as PDFs, EPUBs, YouTube transcripts, RSS feeds, newsletters, and saved webpages remain possible extensions. The original PRD explicitly places these beyond initial URL ingestion.

---

# 5. Product Principles

## 5.1 Source Fidelity

The system must distinguish between:

1. What the source actually says.
2. What the AI explains or infers.
3. What is newly generated as educational material.

Generated explanations must never silently become claims attributed to the original author.

This is a hard architectural invariant.

---

## 5.2 Source Is Immutable

The fetched source and canonical normalized representation are immutable inputs to generation.

AI systems may transform their presentation but must not silently alter their meaning.

Any generated statement that introduces information beyond the source must carry an explicit classification.

---

## 5.3 Agent Drives the Workflow

The active workflow agent is the authoritative semantic orchestrator. In V1 the active agent is OpenCode or `agy`.

The agent decides:

- what stage should run,
- which tool should be invoked,
- in what order,
- with what inputs,
- whether additional analysis is required,
- whether a generated artifact needs revision,
- whether validation has passed,
- whether the lesson is ready.

The application must not independently sequence semantic stages behind the agent's back.

---

## 5.4 Deterministic Execution

The Rust execution layer performs well-defined operations deterministically wherever possible.

For example:

```text
Agent:
"Generate audio for these narration segments using Kokoro."

        ↓

Tool:
generate_audio(...)

        ↓

Rust:
load provider
check cache
generate audio
write files
hash outputs
record metadata
return artifact references
```

The agent decides **what** should happen.

The execution layer decides **how to execute that operation safely and reliably**.

---

## 5.5 Audio Is the Timeline

Audio is the primary synchronization clock.

Text, diagrams, code, charts, and animations are rendered against the audio timeline.

Normal playback must never require an LLM.

This follows the original runtime principle that audio is the primary synchronization clock and the runtime executes without an LLM.

---

## 5.6 Local First

Prefer local execution for:

- source storage,
- generated audio,
- intermediate artifacts,
- metadata,
- playback,
- presentation rendering,
- TTS where practical.

Cloud providers are optional.

The original PRD establishes SQLite + filesystem storage and local-first execution as the initial model.

---

## 5.7 Replaceable Intelligence

The system must not depend on a single agent implementation.

V1 supports exactly two agent providers:

- OpenCode through its native `opencode acp` server,
- Google Antigravity CLI (`agy`) through an ACP adapter.

Other coding-agent providers are deferred. V1 must not build speculative adapters or provider-specific UI for them.

ACP is the V1 client-to-agent control protocol. MCP is the agent-to-Digest tool protocol. The core application exposes stable application services beneath both Tauri commands and MCP handlers.

Hermes Agent becomes the control-plane actor in V2 and consumes the same Digest MCP capabilities. Hermes is not a V1 dependency.

---

## 5.8 Shell Independence

Tauri is an **initial application host**, not an architectural dependency.

The compiler, artifact model, tool interface, manifest, and web runtime must work independently of Tauri.

V1 uses Tauri on Linux to inspect agent activity, evaluate generated output, and play lessons. Tauri commands must delegate to shell-independent Rust application services. No engine, storage, agent-tool, or manifest type may depend on a Tauri type.

This allows the same system to evolve from:

```text
Desktop application
```

to:

```text
Headless Rust service on OCI
```

without rewriting the compiler.

---

## 5.9 Progressive Complexity

V1 prioritizes:

```text
URL
 ↓
Article
 ↓
Understanding
 ↓
Narration
 ↓
Audio
 ↓
Synchronization
 ↓
Presentation
 ↓
Playback
```

Complex AI-generated visualizations are optional and must not block the core learning experience.

---

# 6. Target User

The primary user is a technically sophisticated learner who:

- regularly reads technical articles,
- wants deeper understanding rather than skimming,
- works with developer tooling,
- is comfortable with AI agents,
- prefers local ownership of generated content,
- benefits from audio and visual reinforcement.

Primary job:

> When I find a long technical article, I want to consume it as an engaging narrated presentation so that I can understand it without having to stare at a long page continuously.

The original PRD defines the same primary user and job-to-be-done.

---

# 7. High-Level Architecture

```text
                         ┌──────────────────┐
                         │      User        │
                         └────────┬─────────┘
                                  │
                                  ▼
                    ┌──────────────────────────┐
                    │       Host Application   │
                    │                          │
                    │ Tauri initially          │
                    │ Browser/other hosts later│
                    └────────────┬─────────────┘
                                 │
                                ACP
                                 │
                                 ▼
                    ┌──────────────────────────┐
                    │       ACP Agent          │
                    │                          │
                    │ AUTHORITATIVE             │
                    │ WORKFLOW ORCHESTRATOR    │
                    └────────────┬─────────────┘
                                 │
                              MCP Tool Calls
                                 │
                                 ▼
                    ┌──────────────────────────┐
                    │     Rust Tool Layer      │
                    │                          │
                    │ deterministic execution  │
                    └────────────┬─────────────┘
                                 │
            ┌────────────────────┼────────────────────┐
            │                    │                    │
            ▼                    ▼                    ▼
       Ingestion               TTS              Validation
       Extraction       ┌────────┼────────┐      Alignment
       Normalization    │        │        │      Packaging
                        ▼        ▼        ▼
                     Kokoro   Piper   Google
            │
            ▼
    ┌───────────────────────┐
    │ Artifact Store        │
    │                       │
    │ SQLite + Filesystem   │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ Versioned Manifest    │
    └───────────┬───────────┘
                │
                ▼
    ┌───────────────────────┐
    │ React + TypeScript    │
    │ Web Presentation      │
    │ Runtime               │
    └───────────┬───────────┘
                │
                ▼
          Browser Playback
```

V1 uses this topology to evaluate lesson quality:

```text
Tauri
  ├── ACP client ──► OpenCode ACP
  ├── ACP client ──► agy ACP adapter ──► agy
  ├── React lesson runtime
  └── Rust application services
                       ▲
                       │
                 Digest MCP server
                       ▲
                       │
                     Agent
```

The ACP connection controls the V1 agent session. The MCP connection exposes bounded Digest capabilities to that agent. These are different protocol responsibilities.

---

# 8. Responsibility Boundaries

## 8.1 Workflow Agent

The agent owns **workflow decisions**.

It is responsible for:

- interpreting the user's learning request,
- deciding how to analyze the article,
- deciding narration strategy,
- deciding which concepts need explanation,
- deciding which visuals are useful,
- deciding which tools to invoke,
- deciding whether an artifact should be regenerated,
- interpreting validation results,
- deciding when compilation is complete.

The agent does **not** own durable application state.

---

## 8.2 Host Application

The host application owns:

- ACP connection,
- user interface,
- authentication/configuration required to connect to agents,
- starting and exposing the Digest MCP tool server,
- lifecycle management,
- execution permissions,
- local process management,
- artifact access,
- playback hosting.

Tauri is the V1 Linux implementation of this host. It is an evaluation client, not the V2 control plane.

---

## 8.3 Rust Core

Rust owns deterministic execution.

It provides:

- URL fetching,
- immutable source capture,
- article extraction,
- normalization,
- media discovery and localization,
- Chromium-rendered fallback capture when static extraction is insufficient,
- artifact storage,
- SQLite persistence,
- TTS provider implementations,
- audio processing,
- alignment,
- manifest validation,
- packaging,
- caching,
- integrity checks,
- local HTTP serving.

Rust does **not** decide the semantic order of the educational workflow.

---

## 8.4 Web Runtime

React + TypeScript + Vite owns:

- presentation rendering,
- audio playback,
- synchronization,
- highlighting,
- navigation,
- controls,
- visual transitions,
- responsive layout.

It consumes the manifest.

It does not call an LLM during playback.

V1 verifies the runtime in the Linux Tauri WebKitGTK webview and current Chromium. The runtime should use broadly supported web platform APIs and avoid engine-specific behavior where practical. macOS and Firefox are not V1 test targets.

---

# 9. End-to-End Workflow

The logical compilation pipeline is:

```text
User Request
     ↓
V1 ACP Agent Session
     ↓
capture_source
     ↓
extract_article
     ↓
normalize_article
     ↓
analyze / inspect
     ↓
claim + provenance analysis
     ↓
narration planning
     ↓
narration generation
     ↓
generate_audio
     ↓
align_audio
     ↓
presentation planning
     ↓
manifest generation
     ↓
validation
     ↓
package
     ↓
ready
```

Important:

**This is not a hard-coded Rust pipeline.**

The above represents the available capabilities and normal compilation path.

The active workflow agent is responsible for deciding when and whether to invoke each capability.

---

# 10. User Experience

## 10.1 Input

The user provides:

- one URL,
- optionally multiple URLs,
- optionally a title,
- optionally learning instructions.

Examples:

```text
Explain this like I'm learning distributed systems.

Focus on implementation details.

Don't simplify the Rust code.

Give me visual explanations for difficult concepts.
```

---

## 10.2 Processing

The application opens or attaches to an ACP agent session.

The agent receives the user's request and begins orchestration.

The UI should display:

```text
Article: Building a Distributed Cache

Agent:
  ✓ Fetching article
  ✓ Extracting content
  ✓ Understanding article
  ✓ Building claim map
  → Planning narration
  → Generating audio
  ○ Synchronization
  ○ Presentation
  ○ Validation
```

The progress display reflects agent/tool events rather than pretending that a fixed application-owned pipeline is running.

---

# 11. Article Ingestion

The system must support HTTP/HTTPS URLs.

Ingestion is not a single readability operation. It is a deterministic, inspectable sequence:

```text
safe fetch
    ↓
immutable raw capture
    ↓
readable extraction
    ↓
structural normalization
    ↓
media localization
    ↓
completeness validation
```

The normal path fetches HTML directly. A supervised Chromium capture is used only when the direct response is a client-rendered shell, required content is absent, lazy media cannot otherwise be resolved, or extraction confidence is below the configured threshold.

Ingestion must:

- retrieve page content,
- follow redirects,
- record the redirect chain, final URL, response metadata, and capture time,
- preserve the original response bytes as an immutable artifact,
- extract readable content,
- preserve title,
- preserve author where available,
- preserve publication date where available,
- preserve source URL,
- identify headings,
- preserve paragraphs,
- preserve code blocks,
- preserve lists,
- preserve tables where possible,
- preserve equations and technical notation where possible,
- identify images and figures in reading order,
- preserve author-provided alt text, captions, credits, dimensions, and `srcset` candidates,
- resolve and localize relevant image assets,
- preserve references to unsupported audio, video, canvas, iframe, and downloadable assets,
- identify links,
- identify citations/references where possible,
- produce extraction confidence and completeness diagnostics,
- record partial media failures rather than silently omitting them.

For each localized asset, the system records its original URL, resolved URL, detected MIME type, byte length, content hash, and local artifact reference.

Raw captured HTML is untrusted source material. It must never be inserted into the application DOM without sanitization.

The fetcher and asset downloader must enforce:

- HTTP/HTTPS schemes only,
- SSRF protection across initial requests and redirects,
- response, decompression, timeout, redirect, asset-count, and total-byte limits,
- MIME detection rather than trusting filename extensions,
- SVG and HTML sanitization before rendering,
- no paywall or access-control bypass.

---

# 12. Canonical Article Representation

Raw HTML must be transformed into a normalized representation.

Example:

```json
{
  "schemaVersion": "1.0",
  "article": {
    "id": "article-123",
    "title": "Example Article",
    "author": "...",
    "sourceUrl": "https://example.com/article",
    "publishedAt": "..."
  },
  "sections": [
    {
      "id": "section-1",
      "heading": "...",
      "blocks": [
        {
          "id": "block-1",
          "type": "paragraph",
          "text": "..."
        },
        {
          "id": "block-2",
          "type": "code",
          "language": "rust",
          "text": "..."
        },
        {
          "id": "block-3",
          "type": "image",
          "assetId": "asset-1",
          "altText": "...",
          "caption": "..."
        }
      ]
    }
  ],
  "assets": [
    {
      "id": "asset-1",
      "kind": "image",
      "originalUrl": "https://example.com/diagram.png",
      "resolvedUrl": "https://example.com/diagram.png",
      "artifactId": "sha256:...",
      "mimeType": "image/png",
      "contentHash": "...",
      "width": 1600,
      "height": 900,
      "captureStatus": "localized"
    }
  ]
}
```

The normalized article is the **canonical source representation**.

Once created, it should be immutable.

The canonical source representation contains source-provided media metadata only. Agent-generated OCR, image descriptions, diagram interpretations, and inferred relationships are separate derived artifacts with provenance; they never overwrite source alt text or captions.

---

# 13. Source Fidelity and Provenance

Provenance is a first-class part of the data model.

Every generated educational element should be classifiable as one of:

```text
SOURCE
SOURCE_DERIVED
AI_EXPLANATION
AI_INFERENCE
GENERATED_EDUCATIONAL
```

A generated statement must be traceable to relevant source blocks where applicable.

Example:

```json
{
  "id": "claim-17",
  "type": "source_claim",
  "text": "...",
  "sourceBlocks": ["block-42", "block-43"]
}
```

An explanation:

```json
{
  "id": "explanation-9",
  "type": "ai_explanation",
  "text": "...",
  "supports": ["claim-17"],
  "sourceBlocks": ["block-42", "block-43"]
}
```

The runtime should be able to distinguish these categories.

---

# 14. Article Understanding

The agent should analyze the normalized article and extract structured information including:

- major concepts,
- supporting concepts,
- key claims,
- examples,
- definitions,
- comparisons,
- causal relationships,
- code examples,
- quantitative claims,
- important entities,
- difficult sections,
- prerequisites,
- visualization opportunities,
- likely points of confusion.

The original PRD explicitly requires this structured analysis rather than only natural-language output.

Example:

```json
{
  "concepts": [
    {
      "id": "concept-1",
      "name": "Raft consensus",
      "difficulty": "high",
      "sourceBlocks": ["block-42", "block-43"],
      "visualizationOpportunity": true
    }
  ]
}
```

---

# 15. Narration

The narration system converts article material into spoken educational content.

Narration must:

- preserve source meaning,
- retain important qualifiers,
- avoid unnecessary repetition,
- remain understandable when spoken,
- distinguish explanation from source claims,
- preserve important technical terminology,
- avoid inventing unsupported facts.

Narration modes:

```text
Faithful
Explained
Deep Dive
Executive
```

V1 default:

```text
Faithful + Explained
```

---

# 16. Display Text vs TTS Text

Visible text and spoken text must be separate.

Example:

```json
{
  "displayText": "io_uring provides asynchronous I/O...",
  "ttsText": "eye-oh uring provides asynchronous I/O..."
}
```

This prevents pronunciation normalization from altering the source representation.

---

# 17. TTS Architecture

TTS is exposed through an abstraction:

```rust
trait AudioProvider {
    async fn synthesize(
        &self,
        request: AudioRequest
    ) -> Result<AudioArtifact>;
}
```

Providers:

```text
AudioProvider
 ├── KokoroProvider
 ├── PiperProvider
 └── GoogleTtsProvider
```

## 17.1 Kokoro

Primary local TTS provider.

Use for:

- normal V1 generation,
- higher-quality local narration,
- OCI experimentation.

---

## 17.2 Piper

Low-resource local fallback.

Use when:

- Kokoro is unavailable,
- resource consumption is too high,
- fast synthesis is preferable,
- running under constrained hardware.

---

## 17.3 Google Cloud TTS

Cloud provider.

Use for:

- fallback,
- quality comparison,
- optional higher-quality generation,
- provider benchmarking.

Cloud credentials must never be embedded into manifests.

---

# 18. Audio Segmentation

Audio should be generated in segments rather than only as one monolithic file.

Example:

```text
segment-001
segment-002
segment-003
...
```

Each segment contains:

```text
narration text
audio file
duration
alignment
source references
```

A final concatenated audio file may also be generated.

Segment-level generation allows:

- retries,
- partial regeneration,
- caching,
- provider switching,
- easier synchronization.

---

# 19. Audio Alignment

The alignment layer produces timestamps at multiple granularities where available:

```text
paragraph
sentence
word
```

Preferred:

```text
word > sentence > paragraph
```

Graceful fallback:

```text
No word timestamps
        ↓
Sentence timestamps

No sentence timestamps
        ↓
Paragraph timestamps
```

The runtime must not require perfect word-level alignment to function.

---

# 20. Presentation Planning

The agent decides how the article should be presented.

Possible components:

```text
heading
paragraph
article-text
quote
code
concept-card
diagram
chart
table
timeline
flow
comparison
image
callout
formula
animation
interactive-demo
```

Visual generation should only occur when it improves comprehension.

A difficult technical concept should not automatically result in a visualization.

---

# 21. Presentation Manifest

The manifest is the contract between compilation and playback.

It must be versioned.

Example:

```json
{
  "schemaVersion": "1.0",

  "article": {
    "id": "article-123",
    "title": "Example Article",
    "sourceUrl": "https://example.com/article"
  },

  "audio": {
    "duration": 1872.4,
    "provider": "kokoro",
    "file": "audio/main.opus"
  },

  "segments": [
    {
      "id": "segment-1",

      "start": 0,
      "end": 18.2,

      "sourceBlocks": ["block-1"],

      "provenance": {
        "type": "source"
      },

      "presentation": {
        "type": "article-text",
        "blockIds": ["block-1"]
      },

      "narration": {
        "displayText": "...",
        "ttsText": "...",
        "wordTimings": []
      }
    }
  ]
}
```

The original PRD establishes the manifest as the core generation/runtime contract and requires schema versioning.

V1 changes the wording of the boundary:

> The manifest is the contract between the **agent-driven compiler and the web runtime**, not between the compiler and Tauri specifically.

---

# 22. Presentation Runtime

The runtime is implemented using:

```text
React
+
TypeScript
+
Vite
```

It consumes only the manifest and referenced assets.

Astro is not part of the V1 runtime. The synchronized player is an application with one shared audio-driven state graph rather than a collection of independent interactive islands. Astro may be reconsidered for a future static publishing or reading-mode surface.

It provides:

- audio playback,
- synchronized text,
- word/sentence highlighting,
- automatic scrolling,
- visual transitions,
- section navigation,
- playback speed,
- seeking,
- pause/resume,
- replay,
- keyboard shortcuts.

These runtime requirements follow the original presentation-runtime specification.

---

# 23. Runtime Clock

There must be one authoritative playback clock:

```text
currentTime
     │
     ├── audio
     ├── text highlighting
     ├── presentation state
     ├── diagrams
     ├── code walkthrough
     └── transitions
```

The runtime should derive visual state from this clock.

No LLM should participate in normal playback.

---

# 24. Host Application

## Initial Host

**Tauri on Linux**

Tauri provides:

- desktop packaging,
- local process lifecycle,
- native filesystem integration,
- ACP client hosting,
- local HTTP server lifecycle,
- configuration,
- future background execution.

The V1 Tauri client exists to evaluate:

- extracted-source completeness,
- source and generated-content fidelity,
- narration quality,
- visual usefulness,
- audio quality,
- synchronization,
- end-to-end lesson usability.

However:

> **Tauri must not become a dependency of the compiler architecture.**

The core Rust crates should be usable without Tauri.

V2 replaces the Tauri-hosted agent-control topology with Hermes Agent on a remote Linux server. This migration must reuse the same application services and MCP contracts.

---

# 25. Shell-Independent Core

The project should be structured approximately as:

```text
engine/
    article/
    ingestion/
    normalization/
    analysis/
    narration/
    audio/
    alignment/
    presentation/
    validation/
    artifacts/
    storage/
    jobs/
    tools/

agent/
    acp/
    session/
    events/

runtime/
    manifest/
    playback/

host/
    tauri/
```

The exact repository structure can evolve, but the conceptual separation must remain.

A future headless deployment should be able to reuse:

```text
engine/
agent/
runtime/
```

without depending on Tauri.

---

# 26. V1 Agent Integration: ACP and MCP

V1 uses two complementary protocols:

```text
ACP
  Tauri host controls and observes the agent session

MCP
  Agent invokes bounded Digest tools
```

The host launches:

```text
OpenCode
  command: opencode acp

agy
  command: configured agy ACP adapter
  adapter launches or attaches to agy
```

The ACP client must negotiate protocol version and capabilities before using optional behavior. It must support the subset required by the two V1 providers:

- initialize,
- create session,
- prompt,
- streamed session updates,
- tool-call activity,
- permission requests,
- structured user input where advertised,
- cancellation,
- session load or resume where advertised,
- session close where advertised,
- process exit and protocol error reporting.

Provider identity, provider session identity, Digest job identity, model identity, and adapter version are separate fields. The UI should render a canonical event model while retaining provider-specific metadata for diagnostics.

V1 does not require provider parity. Unsupported optional capabilities must be represented explicitly and handled gracefully.

The Digest MCP server is backed by shell-independent Rust application services. The V1 default transport is stdio. Tool handlers must not call Tauri APIs.

For V1 packaging, the Linux application executable may expose an internal `--digest-mcp` stdio mode in addition to the standalone `digest-mcp` binary. This avoids a platform-specific sidecar bundle while preserving the same MCP server and shell-independent service boundary.

An evaluation run may authorize the agent's advertised one-time permission option only after explicit user consent. Digest must never automatically select a persistent permission option.

In V2, Hermes connects to these same Digest MCP capabilities locally or over an authenticated remote transport. ACP may remain useful for diagnostics or an embedded chat surface, but V2 must not require the V1 Tauri host.

---

# 27. Agent Session Model

A compilation is associated with an agent session.

Conceptually:

```text
User request
     ↓
ACP session
     ↓
Agent reasoning/orchestration
     ↓
Tool calls
     ↓
Artifacts
     ↓
Validation
     ↓
Completion
```

The session can be interrupted.

The application must preserve artifacts independently of the agent's in-memory state.

Therefore recovery is based on:

```text
Persisted artifacts
+
Artifact metadata
+
Agent session context
```

rather than relying entirely on the agent remembering what happened.

---

# 28. Agent Tools

The agent interacts with deterministic application capabilities through the Digest MCP server.

Initial tool set:

```text
capture_source
extract_article
normalize_article

read_artifact
inspect_article
write_analysis
write_narration_plan
write_narration_segments
write_presentation_plan

generate_audio
align_audio

create_diagram
create_chart
create_animation

build_manifest
validate_manifest

preview_segment
package_article
```

`analyze_article`, `extract_claims`, `plan_narration`, and `generate_narration` are agent reasoning responsibilities, not hidden Rust intelligence tools. The agent performs those tasks and submits schema-specific artifacts through validated write tools.

There is no unrestricted `write_artifact` tool in V1. Each write capability accepts a known schema, validates references and provenance, and writes only within the active job's artifact namespace.

Tools must have:

- structured input schemas,
- structured output schemas,
- explicit errors,
- artifact references,
- deterministic behavior where applicable,
- permission boundaries,
- version information,
- job and session correlation,
- bounded resource use,
- idempotency or explicit duplicate semantics.

---

# 29. Tool Responsibility Principle

Tools should perform capabilities.

They should not secretly perform unrelated orchestration.

Bad:

```text
generate_article()
    internally:
       analyze
       narrate
       generate_audio
       validate
```

Preferred:

```text
Agent
 ├── capture_source()
 ├── extract_article()
 ├── reason about source
 ├── write_analysis()
 ├── write_narration_plan()
 ├── write_narration_segments()
 ├── generate_audio()
 ├── align_audio()
 ├── write_presentation_plan()
 ├── build_manifest()
 └── validate_manifest()
```

This preserves the agent's authority over workflow.

---

# 30. Agent Skills

Suggested skills:

```text
skills/
├── article-ingestion/
├── article-analysis/
├── source-fidelity/
├── narration-planning/
├── narration-generation/
├── visualization-planning/
├── diagram-generation/
├── code-explanation/
├── chart-generation/
├── synchronization/
├── presentation-validation/
└── artifact-packaging/
```

Each skill should specify:

- purpose,
- inputs,
- outputs,
- constraints,
- failure conditions,
- examples.

---

# 31. Agent System Prompt

The system prompt must establish:

1. The source is authoritative for source attribution.
2. Source claims and generated explanations must remain distinct.
3. The agent is the workflow orchestrator.
4. Tools perform deterministic application capabilities.
5. The agent must inspect existing artifacts before regenerating them.
6. Existing valid artifacts should be reused.
7. Generated claims must not be presented as source claims.
8. Qualifiers, scope, attribution, and quantitative details must be preserved.
9. The agent must validate the final manifest.
10. The agent should prefer simpler presentations when additional visuals do not improve understanding.

---

# 32. State Ownership

This distinction is critical.

## Agent owns

```text
workflow decisions
semantic planning
educational decisions
tool sequencing
revision decisions
```

## Application owns

```text
durable artifacts
database state
job records
filesystem
credentials/configuration
tool execution
permissions
integrity
```

## Runtime owns

```text
current playback state
visual state
audio synchronization
UI interaction
```

The agent must not become the database.

---

# 33. Durable Job System

Compilation must be represented as a durable job.

The original PRD defines stages and requires each stage to produce an artifact so that retries, debugging, caching, partial regeneration, and inspection are possible.

V1 adapts this to an **agent-driven job model**.

Example:

```text
Job
 │
 ├── artifacts
 │
 ├── agent session
 │
 ├── tool invocations
 │
 ├── validation results
 │
 └── final manifest
```

There is no requirement that the agent execute the stages in a rigid predefined order.

Instead, the job records what the agent actually did.

---

# 34. Artifact Model

Example:

```text
article-123/
│
├── source/
│   ├── original.html
│   └── normalized.json
│
├── analysis/
│   ├── concepts.json
│   ├── claims.json
│   └── provenance.json
│
├── narration/
│   ├── plan.json
│   └── segments.json
│
├── audio/
│   ├── segment-001.opus
│   ├── segment-002.opus
│   └── main.opus
│
├── alignment/
│   └── timings.json
│
├── presentation/
│   └── manifest.json
│
├── validation/
│   └── report.json
│
└── metadata.json
```

This is based on the original artifact model, extended with provenance and validation artifacts.

---

# 35. Storage

Initial storage:

```text
SQLite
+
Filesystem Artifact Store
```

SQLite stores:

- articles,
- jobs,
- agent sessions,
- tool invocations,
- metadata,
- playback progress,
- preferences,
- provider configuration,
- artifact references,
- validation state.

Filesystem stores:

- source content,
- normalized articles,
- audio,
- manifests,
- images,
- generated visual assets,
- intermediate JSON artifacts.

The original PRD establishes this SQLite + filesystem model.

---

# 36. Artifact Integrity

Every artifact should have:

```text
artifact_id
content_hash
created_at
producer
producer_version
input_references
schema_version
```

Artifacts should be immutable once finalized.

Regeneration creates a new artifact version rather than silently overwriting history.

---

# 37. Caching

Every expensive operation should be cacheable.

Cache keys should incorporate:

```text
source content hash
+
model
+
model version
+
prompt/skill version
+
configuration
```

This follows the original caching requirement.

Example:

```text
hash(
    normalizedArticleHash
    +
    narrationModel
    +
    narrationPromptVersion
    +
    voice
    +
    audioSettings
)
```

TTS provider identity and model/version must be part of the cache key.

---

# 38. Recovery

The system must recover from:

- agent interruption,
- tool failure,
- TTS failure,
- alignment failure,
- malformed generated artifacts,
- application restart,
- network failure.

Recovery principle:

```text
Valid artifact exists?
        │
       YES
        ↓
Reuse it

No valid artifact?
        │
        ↓
Agent decides whether to regenerate
```

The application must not discard successful previous work merely because a later stage failed.

---

# 39. Failure Philosophy

The system should degrade gracefully.

Examples:

```text
No visualization
        ↓
Continue with article + audio

No word timestamps
        ↓
Use sentence timestamps

No sentence timestamps
        ↓
Use paragraph timestamps

TTS failure for one segment
        ↓
Retry only that segment

Kokoro unavailable
        ↓
Try Piper

Local TTS unavailable
        ↓
Use Google TTS if configured

Agent interruption
        ↓
Resume from persisted artifacts
```

The original PRD establishes the same granular recovery philosophy.

---

# 40. Validation

Validation occurs at multiple levels.

## Source Validation

Verify:

- source URL,
- extraction completeness,
- normalized structure,
- source metadata.

## Provenance Validation

Verify:

- source claims have source references,
- generated explanations are correctly classified,
- source attribution is not silently altered.

## Narration Validation

Verify:

- required source content is represented,
- important qualifiers are preserved,
- narration segments reference valid blocks.

## Audio Validation

Verify:

- audio exists,
- duration is valid,
- segments are playable.

## Alignment Validation

Verify:

- timestamps are monotonic,
- timestamps fit audio duration,
- referenced segments exist.

## Manifest Validation

Verify:

- schema version,
- valid references,
- valid component types,
- valid timing,
- valid provenance,
- valid assets.

---

# 41. Quality Gates

A lesson is `READY` only when:

```text
source valid
AND
normalized article valid
AND
narration valid
AND
audio valid
AND
alignment valid
AND
manifest valid
AND
required provenance valid
```

Optional visual components may fail without blocking the core lesson.

`READY` means structurally playable; it does not by itself mean educationally good. Every benchmark lesson must also produce a quality report covering:

```text
source text coverage
source structure and media coverage
unsupported or failed captures
unsupported generated claims
qualifier and quantitative-detail preservation
narration clarity and pacing
technical pronunciation
audio defects
alignment errors
visual relevance
overall learning usefulness
```

Deterministic checks are machine-scored. Educational quality is human-reviewed with a fixed rubric. OpenCode and `agy` comparisons must use the same immutable source captures and equivalent generation settings.

---

# 42. Local Web Serving

Generated lessons should be served through a local HTTP server.

Example:

```text
Rust application
      ↓
localhost:PORT
      ↓
React runtime
      ↓
manifest + artifacts
```

The server should serve generated lessons without requiring the ACP agent to remain active.

This is important:

> **Compilation requires the agent. Playback does not.**

---

# 43. Remote Access

Development and personal-device access should use:

```text
Local HTTP Server
       ↓
Cloudflare Tunnel
       ↓
Phone / Tablet Browser
```

The tunnel is a **delivery mechanism**, not part of the compilation architecture.

The generated lesson itself should remain self-contained enough that it can later be served from:

- OCI,
- object storage,
- a CDN,
- or another HTTP server.

---

# 44. Future OCI Deployment

The architecture should eventually support:

```text
OCI Ampere ARM64
       │
       ├── Rust engine
       ├── SQLite
       ├── artifact filesystem
       ├── local TTS
       ├── HTTP server
       └── Hermes Agent control plane
```

The desktop host should not be required for this mode.

This is the V2 target and is not part of the V1 definition of done.

The same compiler should therefore support:

```text
Desktop mode
```

and:

```text
Headless server mode
```

---

# 45. Security

Sensitive configuration must not be written into article manifests.

API keys should use OS-appropriate secure storage where possible.

The application must distinguish:

```text
local models
cloud models
source content
generated content
configuration
```

The user should be able to delete an article and all associated artifacts.

These requirements follow the local-first security model of the original PRD.

---

# 46. Permissions

The agent must not receive unrestricted access to the host.

Tools should expose only the capabilities required by the application.

Examples:

```text
capture_source
    network access

write_analysis
write_narration_plan
write_narration_segments
write_presentation_plan
    schema-validated access to the active job namespace

generate_audio
    TTS/model access

package_article
    artifact-directory access
```

The tool boundary becomes the security boundary between the agent and the application.

The agent does not receive unrestricted filesystem access through Digest. Provider-native tools remain subject to their own sandbox and approval configuration.

---

# 47. Technology Stack

## Core

```text
Rust
```

## Host

```text
V1: Tauri on Linux — evaluation client
V2: Hermes Agent gateway + headless Rust service
```

Tauri is replaceable.

## Agent Protocols

```text
ACP — V1 host-to-agent session control
MCP — agent-to-Digest tool invocation
```

## Agent

Initially:

```text
OpenCode via opencode acp
agy via an ACP adapter
```

V2:

```text
Hermes Agent on a remote Linux server
```

## Frontend

```text
React
TypeScript
Vite
```

## Persistence

```text
SQLite
Filesystem
```

## TTS

```text
Kokoro
Piper
Google Cloud TTS
```

## Delivery

```text
Local HTTP server
Cloudflare Tunnel
```

## Deployment Target

```text
V1: Local Linux desktop
V2: Remote Linux / OCI Ampere ARM64
```

---

# 48. Repository-Level Architectural Boundary

The project should conceptually resemble:

```text
article-learning-engine/
│
├── crates/
│   ├── engine/
│   ├── ingestion/
│   ├── normalization/
│   ├── analysis/
│   ├── narration/
│   ├── audio/
│   ├── alignment/
│   ├── presentation/
│   ├── validation/
│   ├── artifacts/
│   ├── storage/
│   └── tools/
│
├── agent/
│   ├── acp/
│   ├── session/
│   └── events/
│
├── mcp/
│   └── digest-tools/
│
├── runtime/
│   └── web/
│
├── host/
│   └── tauri/
│
├── skills/
│
└── schemas/
    ├── article.schema.json
    ├── analysis.schema.json
    ├── narration.schema.json
    └── manifest.schema.json
```

Exact naming can change.

The architectural separation cannot.

---

# 49. API/Tool Contract Principle

Every tool should expose structured contracts.

Example:

```json
{
  "tool": "generate_audio",
  "input": {
    "segments": [
      {
        "id": "segment-1",
        "ttsText": "..."
      }
    ],
    "provider": "kokoro",
    "voice": "..."
  }
}
```

Result:

```json
{
  "artifacts": [
    {
      "segmentId": "segment-1",
      "artifactId": "audio-segment-1",
      "path": "audio/segment-1.opus",
      "duration": 18.2,
      "contentHash": "..."
    }
  ]
}
```

The agent should operate on references rather than repeatedly transferring large artifacts through its conversation context.

---

# 50. Agent Event Model

The host should expose agent activity to the UI.

Conceptual events:

```text
session_started
agent_thinking
tool_started
tool_progress
tool_completed
artifact_created
validation_started
validation_failed
validation_passed
session_completed
session_failed
```

The UI should present useful progress without requiring the user to understand the underlying ACP protocol.

---

# 51. Observability

Every tool invocation should record:

```text
job_id
session_id
tool_name
input_hash
start_time
end_time
status
error
output_artifacts
```

This allows debugging questions such as:

> Why did this article fail?

or:

> Which stage generated this audio?

---

# 52. Versioning

The following must be versioned independently:

```text
manifest schema
article schema
tool schemas
agent skills
prompts
TTS provider/model
presentation component schemas
```

An artifact must record the versions responsible for producing it.

---

# 53. Testing Strategy

## Unit Tests

Test:

- article parsing,
- normalization,
- manifest validation,
- provenance validation,
- timestamp calculations,
- timeline resolution,
- storage,
- cache keys,
- TTS provider interfaces,
- presentation component validation.

---

## Integration Tests

Test:

```text
URL
 ↓
normalized article
 ↓
agent analysis
 ↓
narration
 ↓
audio
 ↓
alignment
 ↓
manifest
 ↓
playback
```

---

## Golden Tests

Maintain known articles and expected:

- normalized structures,
- claim structures,
- narration boundaries,
- manifest schemas,
- provenance relationships.

---

## Playback Tests

Verify:

- play,
- pause,
- seek,
- resume,
- speed changes,
- section navigation,
- synchronization.

---

## Agent Tests

Use fixture articles to evaluate:

- source fidelity,
- workflow completion,
- visualization decisions,
- manifest validity,
- unnecessary hallucination,
- explanation quality,
- tool selection,
- artifact reuse,
- recovery behavior.

The original PRD similarly calls for golden, playback, integration, and agent tests.

---

# 54. Benchmark Corpus

V1 should use a small fixed benchmark set containing:

1. A straightforward technical article.
2. A long systems article.
3. An article containing Rust code.
4. An article containing quantitative claims.
5. An article with diagrams, captions, and tables.
6. An article with equations or technical notation.
7. An article using lazy-loaded images and `srcset`.
8. A client-rendered article that requires Chromium fallback.
9. An article with ambiguous explanations.
10. An article with substantial technical terminology.

For each benchmark, measure:

```text
source text coverage
source structure coverage
source media coverage
unsupported or failed captures
source fidelity
narration quality
unsupported narration claims
audio quality
alignment quality
manifest validity
presentation usefulness
generation time
resource usage
```

Each benchmark run emits a machine-readable quality report plus a human review score. V1 quality decisions must compare outputs from fixed source captures so extraction changes and agent changes can be evaluated independently.

---

# 55. V1 Development Phases

## Phase 0 — Walking Skeleton

Deliver:

- repository structure,
- Tauri host,
- shell-independent Rust application service boundary,
- minimal SQLite and artifact store,
- versioned artifact envelope,
- OpenCode ACP connection,
- two-tool Digest MCP server,
- canonical agent event stream visible in Tauri.

Exit criteria:

> OpenCode can start through ACP, invoke a Digest MCP tool, create a validated artifact through a shell-independent Rust service, and expose the event and artifact in Tauri.

---

## Phase 1 — Quality Evaluation Vertical Slice

Deliver:

- one article URL input,
- immutable HTML source capture,
- normalized paragraphs, headings, code, images, and callouts,
- localized image assets,
- minimal agent analysis and narration artifacts,
- one local TTS provider,
- paragraph or sentence timing,
- minimal versioned manifest,
- synchronized playback in Tauri.

Exit criteria:

> One benchmark article becomes a complete playable lesson through an OpenCode-orchestrated workflow, and the run emits a quality report.

### Current implementation checkpoint

The first Phase 1 increment now provides:

- article URL input in the Tauri control plane,
- bounded native HTTP capture with manual redirect handling and rejection of private or loopback targets,
- immutable raw response objects plus versioned source-capture envelopes,
- a normalized article artifact containing stable block IDs, headings, paragraphs, code blocks, lists, quotes, and resolved image references,
- main-content candidate scoring plus persisted confidence, coverage counts, and extraction warnings,
- source image metadata including captions, dimensions, and resolved `srcset` candidates,
- bounded image localization with byte-derived MIME detection, immutable image artifacts, and explicit partial-failure records,
- an `ingest_article` MCP tool backed by the same shell-independent Rust application service,
- a `write_narration_plan` MCP tool that keeps display text and TTS text separate and requires source-block provenance,
- machine-readable narration presentation types in the MCP schema so agents can correct invalid tool arguments,
- durable agent attempts with terminal status and provider-session linkage,
- a five-minute ACP run deadline and host-startup recovery of attempts interrupted by a previous process,
- raw canonical agent events plus a coalesced presentation-event projection,
- a dark Chromium/WebView-oriented evaluation interface with durable recent-run navigation, capture-quality summaries, and media-localization summaries.

This checkpoint does **not** complete Phase 1. The next quality slice must turn narration plans into segmented audio and timing artifacts, define the playable manifest, integrate Kokoro as the primary local TTS provider, and render synchronized playback. Current readability scoring and media selection are intentionally conservative first passes; richer boilerplate removal, responsive-candidate selection, Chromium fallback, a fixture corpus, DNS-rebinding defenses, and format-specific media decoding limits remain Phase 2 work.

---

## Phase 2 — Ingestion Hardening

Deliver:

- safe HTTP fetching and redirect handling,
- structural extraction,
- complete canonical article schema,
- media metadata and localization,
- extraction diagnostics,
- Chromium fallback,
- ingestion fixture corpus,
- source and asset security limits.

Exit criteria:

> The benchmark corpus produces immutable normalized articles with measured text, structure, and media coverage, and every omission or capture failure is reported.

---

## Phase 3 — OpenCode Compilation Quality

Deliver:

- complete OpenCode ACP lifecycle,
- validated Digest MCP tool set,
- article analysis,
- claims and provenance,
- narration planning and generation,
- presentation planning,
- artifact reuse and revision,
- quality comparison across the benchmark corpus.

Exit criteria:

> OpenCode can drive the workflow from source capture to validated presentation artifacts without the application hard-coding semantic sequencing.

---

## Phase 4 — agy Provider

Deliver:

- supervised agy ACP adapter lifecycle,
- capability and version detection,
- authentication guidance,
- canonical event translation,
- cancellation and recovery,
- the same benchmark runs used for OpenCode.

Exit criteria:

> `agy` completes the same vertical slice through the same Digest MCP contracts, with provider differences represented as capabilities rather than branching engine behavior.

---

## Phase 5 — Audio and Synchronization

Deliver:

- AudioProvider abstraction,
- Kokoro primary provider,
- Piper fallback,
- optional Google Cloud TTS,
- segment generation and caching,
- word/sentence/paragraph alignment fallback,
- timing and audio validation,
- segment-level regeneration.

Exit criteria:

> Audio and presentation segments remain synchronized during playback, seeking, speed changes, and segment regeneration.

---

## Phase 6 — Presentation Runtime and Visual Quality

Deliver:

- React + TypeScript + Vite runtime,
- manifest loader and component registry,
- audio player and synchronized text,
- navigation and responsive controls,
- visualization planning,
- diagrams,
- charts,
- code walkthroughs,
- optional animations.

Exit criteria:

> A lesson plays without an active agent in Linux Tauri and current Chromium, and benchmark review shows that visuals improve rather than distract from comprehension.

---

## Phase 7 — Remote Access

Deliver:

- local HTTP serving,
- Cloudflare Tunnel integration/documentation,
- phone/tablet playback.

Exit criteria:

> A generated lesson can be opened and played from another device.

---

## Phase 8 — V2 Readiness, Not V1 Hermes Integration

Deliver:

- verification that engine services run without Tauri,
- transport-independent Digest MCP handlers,
- documented headless startup requirements,
- ARM64 and remote-storage risks recorded for V2.

Exit criteria:

> A V2 Hermes integration can replace the V1 actor and host topology without changing application-service, artifact, manifest, or playback contracts.

Hermes implementation and production OCI deployment are explicitly outside V1.

---

# 56. Definition of Done — V1

V1 is complete when:

### Ingestion

- [ ] User can submit an article URL.
- [ ] Original response bytes and fetch metadata are preserved immutably.
- [ ] Article text, structure, and relevant media are extracted.
- [ ] Images retain source metadata and localized artifact references where captured.
- [ ] Static extraction can escalate to supervised Chromium capture.
- [ ] Extraction completeness and partial failures are reported.
- [ ] Canonical normalized representation is persisted.
- [ ] Original source is preserved.

### Agent

- [ ] OpenCode connects through native ACP.
- [ ] `agy` connects through its configured ACP adapter.
- [ ] Provider capabilities and versions are recorded.
- [ ] Agent can create, cancel, and recover or resume a session where supported.
- [ ] Agent receives user instructions.
- [ ] Agent can invoke validated Digest tools through MCP.
- [ ] Agent drives the compilation workflow.
- [ ] Application does not secretly orchestrate semantic stages.

### Source Fidelity

- [ ] Source claims are represented structurally.
- [ ] Generated explanations are distinguished from source claims.
- [ ] Provenance reaches presentation segments.
- [ ] Important qualifiers are preserved.

### Narration

- [ ] Agent can create narration plans.
- [ ] Narration is segmented.
- [ ] Display text and TTS text are separate.

### TTS

- [ ] Kokoro works.
- [ ] Piper works.
- [ ] Google TTS works when configured.
- [ ] Provider abstraction exists.
- [ ] Audio is cached.
- [ ] Segment-level regeneration works.

### Synchronization

- [ ] Audio timestamps are generated.
- [ ] Word/sentence/paragraph fallback exists.
- [ ] Timeline validation works.

### Presentation

- [ ] Manifest is versioned.
- [ ] Manifest validation works.
- [ ] React runtime consumes the manifest.
- [ ] Audio and visual state remain synchronized.
- [ ] Playback requires no LLM.

### Persistence

- [ ] SQLite stores application metadata.
- [ ] Filesystem stores artifacts.
- [ ] Jobs are durable.
- [ ] Artifacts are hashed.
- [ ] Existing valid artifacts can be reused.

### Recovery

- [ ] Individual TTS segments can be retried.
- [ ] Failed tools expose structured errors.
- [ ] Previous artifacts survive failure.
- [ ] Agent sessions can resume from persisted state.

### Delivery

- [ ] Lessons are served locally.
- [ ] Lessons can be opened from a phone/browser.
- [ ] Cloudflare Tunnel can expose the local server.
- [ ] Linux Tauri WebKitGTK and current Chromium playback are tested.

### Architecture

- [ ] Core does not depend on Tauri.
- [ ] Web runtime does not depend on Tauri.
- [ ] Agent is replaceable.
- [ ] TTS providers are replaceable.
- [ ] Compiler can eventually run headlessly.
- [ ] Tauri commands and MCP tools delegate to the same shell-independent application services.
- [ ] No V1 engine, artifact, manifest, or playback contract depends on Tauri.
- [ ] The V2 Hermes actor can reuse the Digest MCP contract without changing compiler semantics.

### Quality Evaluation

- [ ] A fixed benchmark corpus is versioned.
- [ ] Every benchmark run produces a machine-readable quality report.
- [ ] Human review covers source fidelity, narration, visuals, audio, synchronization, and learning usefulness.
- [ ] OpenCode and `agy` outputs can be compared from the same immutable source captures.

---

# 57. Core Architectural Invariants

These must remain true throughout development.

### Invariant 1 — Agent Authority

> **The active workflow agent is the authoritative semantic orchestrator for article compilation.**

In V1 the active agent is OpenCode or `agy`, controlled through ACP. In V2 it is Hermes. The application provides MCP capabilities; the agent decides how those capabilities are composed.

---

### Invariant 2 — Deterministic Execution

> **The Rust core executes tool operations deterministically and records their outputs as durable artifacts.**

---

### Invariant 3 — Source Fidelity

> **The source is immutable truth; intelligence may transform its presentation but may not silently transform its meaning.**

---

### Invariant 4 — Manifest Boundary

> **The manifest is the stable contract between compilation and playback.**

---

### Invariant 5 — Runtime Independence

> **Normal playback must not require an agent or LLM.**

---

### Invariant 6 — Shell Independence

> **Tauri is the V1 evaluation host, not the architecture or the V2 control plane.**

The compiler must remain usable without it.

---

### Invariant 7 — Durable State

> **Application state lives in persistent artifacts and storage, not solely inside the agent session.**

---

### Invariant 8 — Replaceability

> **Agent providers, TTS providers, visualization systems, and host applications must be replaceable behind stable interfaces.**

---

### Invariant 9 — V1/V2 Migration Seam

> **V2 replaces the actor and deployment topology, not the Rust application services, Digest MCP tools, artifact schemas, manifest, or player.**

---

# 58. Final Architecture

The V1 quality-evaluation architecture is therefore:

```text
                         USER
                           │
                           ▼
                ┌─────────────────────┐
                │     HOST            │
                │                     │
                │ Tauri initially     │
                │ shell-independent   │
                └──────────┬──────────┘
                           │
                          ACP
                           │
                           ▼
                ┌─────────────────────┐
                │   ACP CONNECTED     │
                │       AGENT         │
                │                     │
                │ WORKFLOW AUTHORITY  │
                └──────────┬──────────┘
                           │
                       MCP tools
                           │
                           ▼
                ┌─────────────────────┐
                │     RUST CORE       │
                │                     │
                │ deterministic       │
                │ execution           │
                └──────────┬──────────┘
                           │
          ┌────────────────┼─────────────────┐
          │                │                 │
          ▼                ▼                 ▼
       Article           Audio          Presentation
       Pipeline          Pipeline          Pipeline
          │                │                 │
          │        ┌───────┼───────┐         │
          │        ▼       ▼       ▼         │
          │     Kokoro   Piper   Google      │
          │
          └────────────────┬─────────────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │   ARTIFACT STORE    │
                │                     │
                │ SQLite + Filesystem │
                └──────────┬──────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │ VERSIONED MANIFEST  │
                └──────────┬──────────┘
                           │
                           ▼
                ┌─────────────────────┐
                │ REACT + TYPESCRIPT  │
                │ WEB RUNTIME         │
                └──────────┬──────────┘
                           │
                           ▼
                 ┌──────────────────┐
                 │ BROWSER / PHONE  │
                 └──────────────────┘
```

The most important boundary is:

```text
              ┌──────────────────────────┐
              │          AGENT           │
              │                          │
              │ "What should happen?"    │
              └────────────┬─────────────┘
                           │
                       TOOL API
                           │
              ┌────────────▼─────────────┐
              │       RUST CORE          │
              │                          │
              │ "How should it happen    │
              │  reliably?"              │
              └────────────┬─────────────┘
                           │
                       ARTIFACTS
                           │
              ┌────────────▼─────────────┐
              │        MANIFEST          │
              │                          │
              │ "What was produced?"     │
              └────────────┬─────────────┘
                           │
              ┌────────────▼─────────────┐
              │       WEB RUNTIME        │
              │                          │
              │ "How should it play?"    │
              └──────────────────────────┘
```

This architecture preserves the original PRD's core principles—source fidelity, audio-driven synchronization, deterministic playback, local-first storage, durable artifacts, and replaceable intelligence—while making the **agent-driven orchestration model** explicit and removing Tauri as an architectural constraint.

The planned V2 topology is:

```text
User channels / schedules
          │
          ▼
Hermes Agent on remote Linux
          │
     Digest MCP tools
          │
          ▼
Headless Rust application services
          │
          ├── SQLite + artifact store
          ├── TTS + alignment
          └── manifest + HTTP serving
                         │
                         ▼
                  React web runtime
```

Hermes integration is not part of V1 implementation. This topology is recorded now only to protect the migration seam.

# 59. North-Star Statement

> **The active agent compiles source material into a provenance-aware, synchronized multimedia lesson manifest through Digest MCP tools. The Rust core provides the reliable execution substrate. The web runtime deterministically plays the resulting lesson. V1 evaluates quality through Tauri with OpenCode and `agy`; V2 replaces that control topology with Hermes without rewriting the engine.**

That is the V1 architecture we should implement against.

This is the version I would treat as the **implementation baseline**. The major architectural correction from the earlier PRD is that the pipeline is now explicitly **agent-orchestrated rather than application-orchestrated**, while Tauri has been reduced to an initial host.
