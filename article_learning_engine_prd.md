# PRD — Tauri AI Article Learning & Presentation Engine

**Status:** Draft  
**Version:** 0.1  
**Product Type:** Local-first desktop application  
**Primary Platform:** Tauri + Rust + Web UI  
**Primary User:** A technically curious reader who consumes long-form technical articles but struggles with sustained attention  
**Core Concept:** Convert articles into synchronized, narrated, interactive learning experiences where audio, text, code, diagrams, visualizations, and other presentation components advance together on a deterministic timeline.

---

# 1. Executive Summary

This product is a local-first desktop application that transforms web articles into interactive, audio-synchronized learning experiences.

The user pastes one or more article URLs. An agentic content pipeline retrieves and understands the article, creates a narration plan, generates audio using an available local or remote audio model, aligns the narration with the source content, and optionally creates visual demonstrations such as diagrams, charts, code walkthroughs, animations, and concept cards.

The resulting artifact is not merely an audiobook or summarized article. It is an **AI-generated interactive presentation of the article**.

The core UX should feel similar to watching a high-quality educational YouTube video:

- The article is being narrated.
- The relevant text is highlighted and automatically scrolled.
- Important concepts can trigger visual demonstrations.
- Code can be displayed and stepped through.
- Diagrams can be generated for abstract concepts.
- Charts can explain quantitative claims.
- The user can pause, rewind, change speed, jump between sections, or return to the original article.
- Every visual and text state is synchronized to the audio timeline.

The Tauri application acts as the **control plane and deterministic presentation runtime**. An external agent harness accessed through ACP acts as the **planning and orchestration layer**. The agent uses skills and tools to produce a versioned presentation artifact consumed by the Tauri runtime.

---

# 2. Problem Statement

## 2.1 User Problem

Long-form technical articles are valuable but difficult to consume consistently.

Common problems:

- Articles are often thousands of words long.
- Reading requires sustained attention.
- Technical concepts may require additional research.
- Important diagrams or examples may be separated from the explanation.
- Users may lose context when switching between the article, documentation, code, and visualizations.
- Passive audio alone is insufficient for technical material because visual information matters.
- Traditional text-to-speech simply reads the page without adapting the presentation to the content.
- Traditional summaries remove too much detail.
- Video versions of articles are expensive or unavailable.

The desired experience is:

> "Take the article I want to learn and turn it into something I can watch, listen to, and interact with."

## 2.2 Product Opportunity

Modern AI agents can reason over source material, use tools, generate structured content, create code and diagrams, and orchestrate local models.

This allows an article to be treated as source material for a generated presentation rather than as a static webpage.

The system can therefore bridge:

**Article → Understanding → Narration → Visualization → Synchronized Presentation**

---

# 3. Product Vision

Build a personal **AI technical learning engine** that converts written technical material into adaptive, synchronized, multimedia lessons.

The long-term product should make consuming a difficult technical article feel closer to:

- watching a well-produced technical YouTube video,
- listening to an intelligent podcast,
- reading the original source,
- and interacting with an educational visualization,

at the same time.

The system should preserve the author's meaning while making difficult material easier to understand.

---

# 4. Product Principles

## 4.1 Source Fidelity

The system must distinguish between:

1. What the source actually says.
2. What the AI infers or explains.
3. What is newly generated as educational material.

Generated explanations must not silently become claims attributed to the original author.

## 4.2 Audio Is the Timeline

The audio timeline is the primary synchronization clock.

Text, diagrams, code, charts, and animations are rendered against this timeline.

The UI should never depend on an LLM making real-time playback decisions.

## 4.3 Agent Plans, Runtime Executes

The agent should determine **what the presentation should contain**.

The deterministic runtime should determine **how the presentation plays**.

The LLM must not be required during normal playback.

## 4.4 Local-First

Prefer local execution for:

- article storage,
- generated audio,
- intermediate artifacts,
- embeddings,
- metadata,
- playback,
- presentation rendering,
- model execution where practical.

Cloud services may be used as optional providers.

## 4.5 Replaceable Intelligence Layer

The product must not depend on a single agent harness.

Potential agent backends include:

- Codex
- Antigravity
- Claude-based harnesses
- custom ACP-compatible agents
- future local agents

The Tauri application should communicate with an abstract ACP client interface.

## 4.6 Progressive Complexity

The initial product should solve:

**URL → article → narration → synchronization → playback**

before attempting complex AI-generated visuals.

---

# 5. Target User

## Primary Persona

A software/technology-focused learner who:

- reads technical articles regularly,
- saves many articles but does not finish them,
- wants to understand concepts rather than merely skim them,
- is comfortable with developer tooling,
- has access to local models and developer agents,
- prefers owning their generated content locally,
- benefits from audio and visual reinforcement.

## Jobs To Be Done

### Primary

"When I find a long technical article, I want to consume it as an engaging narrated presentation so that I can understand it without having to stare at a long page continuously."

### Secondary

"When a concept is difficult, I want the system to explain it visually instead of only reading the paragraph."

### Tertiary

"When I finish the presentation, I want to retain the important concepts and be able to revisit them later."

---

# 6. Core User Experience

## 6.1 Input

User pastes:

- one URL,
- multiple URLs,
- optionally an article title,
- optionally an instruction such as:
  - "Explain this like I'm learning distributed systems."
  - "Focus on implementation details."
  - "Don't simplify the Rust code."
  - "Give me visual explanations for the difficult concepts."

## 6.2 Processing

The application creates a generation job.

```text
URL
 ↓
Fetch
 ↓
Extract
 ↓
Normalize
 ↓
Analyze
 ↓
Plan narration
 ↓
Plan presentation
 ↓
Generate audio
 ↓
Align audio
 ↓
Validate
 ↓
Build manifest
 ↓
Ready for playback
```

## 6.3 Playback

The user sees:

```text
┌──────────────────────────────────────────────────┐
│ Article title                                    │
├───────────────┬──────────────────────────────────┤
│ Previous      │                                  │
│ Articles      │       Presentation Canvas        │
│               │                                  │
│ Article A     │   highlighted text / diagram     │
│ Article B     │   / code / chart / animation     │
│ Article C     │                                  │
│               │                                  │
├───────────────┴──────────────────────────────────┤
│ ▶  ━━━━━━━━━━━━━━━━━━━  12:43 / 31:20   1.25x   │
└──────────────────────────────────────────────────┘
```

---

# 7. Functional Requirements

## 7.1 Article Ingestion

The system must:

- accept HTTP/HTTPS article URLs,
- retrieve page content,
- handle redirects,
- extract readable article content,
- preserve title, author, publication date, and source URL when available,
- identify headings and sections,
- preserve paragraphs,
- preserve code blocks,
- preserve lists,
- preserve tables where possible,
- identify images,
- identify links,
- identify citations/references where possible.

### Future

Support:

- PDFs,
- local Markdown,
- EPUB,
- saved webpages,
- YouTube transcripts,
- RSS feeds,
- newsletters.

---

# 8. Article Normalization

Raw HTML must be transformed into a normalized representation.

Example:

```json
{
  "article": {
    "title": "...",
    "author": "...",
    "sourceUrl": "...",
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
        }
      ]
    }
  ]
}
```

The normalized article must become the canonical source representation.

---

# 9. Article Understanding

The agent analyzes the normalized article and extracts:

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
- likely visualization opportunities,
- likely points of confusion.

The analysis should be represented as structured data rather than only natural-language output.

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

# 10. Narration Engine

The narration layer converts source material into spoken content.

## Requirements

The narration should:

- preserve source meaning,
- avoid unnecessary repetition,
- sound conversational,
- pronounce technical terms correctly,
- explain references when appropriate,
- identify when the source is making a claim versus when the narrator is providing additional context,
- support section-level generation,
- support regeneration of individual segments.

## Narration Modes

### Faithful

Closely follows the article.

### Explained

Adds concise explanations around difficult concepts.

### Deep Dive

Adds more educational context and examples.

### Executive

Shorter high-level explanation.

The default should be **Faithful + Explained**.

---

# 11. Audio Generation

The system should support pluggable audio providers.

```text
AudioProvider
 ├── LocalTTS
 ├── CloudTTS
 └── CustomTTS
```

Provider interface should expose:

```text
generate(text, voice, options)
→ AudioArtifact
```

Preferred output:

- WAV for intermediate processing,
- compressed format such as Opus/AAC for playback when appropriate,
- sample rate and channel configuration defined by the pipeline.

The pipeline should support:

- voice selection,
- speaking speed,
- pronunciation hints,
- chunk generation,
- retries,
- caching.

---

# 12. Audio Alignment

Audio alignment is a core feature.

The system should support:

### Level 1

Paragraph timestamps.

### Level 2

Sentence timestamps.

### Level 3

Word timestamps.

Preferred final experience:

**word-level synchronization.**

Example:

```json
{
  "text": "Rust provides memory safety.",
  "words": [
    {"text": "Rust", "start": 0.00, "end": 0.31},
    {"text": "provides", "start": 0.31, "end": 0.72},
    {"text": "memory", "start": 0.72, "end": 1.05},
    {"text": "safety.", "start": 1.05, "end": 1.42}
  ]
}
```

If the TTS provider does not provide timestamps, use a separate alignment stage.

---

# 13. Presentation Planner

This is the central AI capability beyond TTS.

The agent should decide how each section should be presented.

Possible presentation types:

```text
TEXT
PARAGRAPH
HEADING
QUOTE
CODE
DIAGRAM
CHART
TABLE
TIMELINE
FLOW
CONCEPT_CARD
COMPARISON
ANIMATION
IMAGE
CALLOUT
FORMULA
INTERACTIVE_DEMO
```

The planner produces a presentation sequence.

Example:

```json
{
  "segments": [
    {
      "id": "segment-1",
      "sourceBlocks": ["block-1", "block-2"],
      "presentation": {
        "type": "article-text"
      }
    },
    {
      "id": "segment-2",
      "sourceBlocks": ["block-3"],
      "presentation": {
        "type": "diagram",
        "component": "consensus-flow"
      }
    },
    {
      "id": "segment-3",
      "sourceBlocks": ["block-4"],
      "presentation": {
        "type": "code",
        "language": "rust"
      }
    }
  ]
}
```

---

# 14. Visual Explanation System

The system should not generate visuals merely because it can.

Visuals should be generated when they improve comprehension.

## Appropriate Uses

### Architecture

```text
Client
  ↓
API
  ↓
Service
  ↓
Database
```

### Processes

```text
Request
 ↓
Validation
 ↓
Queue
 ↓
Worker
 ↓
Storage
```

### Algorithms

Animated state transitions.

### Distributed systems

- nodes,
- messages,
- elections,
- replication,
- partitions,
- consensus,
- leader changes.

### Programming

- execution flow,
- memory layout,
- data structures,
- ownership,
- call stacks,
- concurrency.

### Business

- market structure,
- flywheels,
- revenue models,
- timelines,
- competitive positioning.

### Quantitative Articles

Charts generated from source data.

---

# 15. Visual Generation Architecture

Visuals should be represented as structured presentation components wherever possible.

Prefer:

```json
{
  "type": "diagram",
  "component": "network-topology",
  "props": {
    "nodes": [...],
    "edges": [...]
  }
}
```

over:

```text
AI-generated PNG
```

Structured visuals provide:

- deterministic rendering,
- accessibility,
- responsiveness,
- animation,
- interaction,
- lower storage,
- easier regeneration.

Raster images can still be supported for cases where generated imagery is appropriate.

---

# 16. Presentation Runtime

The presentation runtime consumes a manifest.

It should provide:

- timeline synchronization,
- audio playback,
- text highlighting,
- automatic scrolling,
- visual transitions,
- section navigation,
- playback speed,
- seeking,
- pause/resume,
- replay,
- keyboard shortcuts.

The runtime must not require an LLM.

---

# 17. Article Manifest

The article manifest is the core contract between the generation system and Tauri.

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
    "provider": "local-tts",
    "file": "audio/main.opus"
  },
  "segments": [
    {
      "id": "segment-1",
      "start": 0,
      "end": 18.2,
      "sourceBlocks": ["block-1"],
      "presentation": {
        "type": "article-text",
        "blockIds": ["block-1"]
      },
      "narration": {
        "text": "...",
        "wordTimings": []
      }
    }
  ]
}
```

The manifest should be versioned.

---

# 18. Synchronization Model

The runtime should maintain a single playback clock:

```text
currentTime
    │
    ├── audio
    ├── active segment
    ├── active paragraph
    ├── active sentence
    ├── active word
    └── presentation state
```

At any point:

```text
currentTime = 743.82
```

the runtime determines:

```text
active segment
active source block
active word
active visualization state
```

This should be deterministic.

---

# 19. Visual Timeline

Visual components may have their own timeline.

Example:

```json
{
  "type": "animation",
  "start": 50.0,
  "duration": 12.0,
  "keyframes": [
    {
      "time": 0,
      "state": "leader-election-start"
    },
    {
      "time": 5,
      "state": "candidate"
    },
    {
      "time": 10,
      "state": "leader"
    }
  ]
}
```

This allows educational animations to remain synchronized with narration.

---

# 20. UI Requirements

## 20.1 Main Screen

Components:

- article library/sidebar,
- article title,
- presentation canvas,
- playback controls,
- progress bar,
- playback speed,
- current section,
- optional source link.

## 20.2 Article Library

Each article should show:

- title,
- source,
- generation status,
- duration,
- progress,
- last played timestamp,
- generation date.

Statuses:

```text
Queued
Fetching
Extracting
Analyzing
Generating
Aligning
Rendering
Ready
Failed
```

## 20.3 Playback Controls

Minimum:

- play/pause,
- seek,
- previous section,
- next section,
- playback speed,
- volume,
- fullscreen.

Future:

- rewind 10 seconds,
- forward 10 seconds,
- repeat section,
- bookmarks.

---

# 21. Generation Job System

Generation should be treated as a durable job.

Example:

```text
Job
 ├── ingest
 ├── normalize
 ├── analyze
 ├── narrate
 ├── audio
 ├── align
 ├── presentation
 ├── validate
 └── package
```

Each stage should produce an artifact.

This allows:

- retries,
- debugging,
- caching,
- partial regeneration,
- inspection.

---

# 22. Artifact Model

Example directory:

```text
article-123/
├── source/
│   ├── original.html
│   └── normalized.json
│
├── analysis/
│   ├── concepts.json
│   └── claims.json
│
├── narration/
│   ├── plan.json
│   └── segments.json
│
├── audio/
│   ├── segment-001.wav
│   ├── segment-002.wav
│   └── main.opus
│
├── alignment/
│   └── timings.json
│
├── presentation/
│   └── manifest.json
│
└── metadata.json
```

---

# 23. Agent Architecture

The agent is an orchestration layer.

It should have:

- system prompt,
- skills,
- tools,
- access to project artifacts,
- access to audio generation,
- access to visualization tools,
- access to validation tools.

The agent should not own application state.

---

# 24. ACP Integration

Tauri should contain an ACP client abstraction.

Example:

```rust
trait AgentProvider {
    async fn start_session(&self, request: SessionRequest) -> Result<Session>;
    async fn send_message(&self, session: &Session, message: AgentMessage) -> Result<AgentResponse>;
    async fn cancel(&self, session: &Session) -> Result<()>;
}
```

Providers can include:

```text
CodexProvider
AntigravityProvider
CustomACPProvider
```

The UI should not know which provider is active.

---

# 25. Agent Skills

Suggested skills:

```text
skills/
├── article-ingestion/
├── article-analysis/
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

Each skill should define:

- purpose,
- inputs,
- outputs,
- constraints,
- failure conditions,
- examples.

---

# 26. Agent Tools

Potential tools:

```text
fetch_url
extract_article
read_artifact
write_artifact
analyze_article
generate_narration
generate_audio
align_audio
create_diagram
create_chart
create_animation
validate_manifest
preview_segment
package_article
```

Tools should expose structured interfaces.

---

# 27. System Prompt Requirements

The system prompt should establish:

1. The source article is authoritative for attribution.
2. The agent must distinguish source claims from generated explanations.
3. Visualizations should be used only when educationally useful.
4. The narration must remain faithful.
5. The agent should avoid unnecessary verbosity.
6. Every presentation component must map to source content or clearly be labeled as supplementary.
7. Generated visuals must be deterministic and serializable where possible.
8. The final output must conform to the manifest schema.
9. The agent should validate its work before completion.

---

# 28. Educational Modes

The user should eventually be able to choose:

## Reader

Minimal transformation.

## Narrated

Article + audio synchronization.

## Explained

Article + narration + explanations.

## Visual

Article + narration + visual demonstrations.

## Deep Learning

Maximum explanation and visualization.

Default:

**Explained**

---

# 29. Personalization

Future versions should support user learning preferences.

Examples:

```text
technicalDepth: high
visualPreference: high
audioSpeed: 1.15
verbosity: medium
codeDetail: high
backgroundKnowledge:
  - Rust
  - TypeScript
  - distributed systems
```

The agent can use these preferences when generating presentations.

---

# 30. Knowledge Continuity

A long-term feature is remembering what the user has already consumed.

The system could maintain:

```text
Concept Graph

Rust
 ├── ownership
 ├── borrowing
 ├── lifetimes
 └── async

Distributed Systems
 ├── consensus
 ├── Raft
 ├── replication
 └── partition tolerance
```

When processing a new article, the agent could recognize known concepts and spend more time on unfamiliar ones.

This feature should come after the core playback system.

---

# 31. Search and Retrieval

Future versions may provide semantic retrieval across consumed articles.

Example:

> "Show me everything I've read about consensus."

The system retrieves relevant article sections and concepts.

Potential implementation:

- local embeddings,
- SQLite metadata,
- vector index,
- full-text search.

---

# 32. Storage

Recommended initial storage:

```text
SQLite
+
filesystem artifact store
```

SQLite stores:

- articles,
- jobs,
- metadata,
- playback progress,
- preferences,
- provider configuration,
- artifact references.

Filesystem stores:

- audio,
- manifests,
- images,
- generated visual assets,
- raw source content.

---

# 33. Local-First Security

Sensitive local configuration should not be written into article manifests.

API keys should be stored using OS-appropriate secure storage where possible.

The application should clearly distinguish:

- local models,
- cloud models,
- source content,
- generated content.

The user should be able to delete an article and all associated artifacts.

---

# 34. Caching

Every expensive generation stage should be cacheable.

Cache keys should incorporate:

- source content hash,
- model,
- model version,
- prompt/skill version,
- generation configuration.

Example:

```text
hash(
  normalizedArticle +
  narrationModel +
  narrationPromptVersion +
  voice +
  audioSettings
)
```

This allows safe regeneration when prompts or models change.

---

# 35. Error Handling

Failures must be visible and recoverable.

Examples:

### Extraction failure

```text
Could not extract article content.
Try opening the source manually or providing another URL.
```

### TTS failure

Allow retry of only the failed audio segment.

### Alignment failure

Allow fallback to sentence-level synchronization.

### Visualization failure

Continue without the visualization.

### Agent failure

Preserve all completed artifacts and allow resuming.

The entire article should not need to be regenerated because one visualization failed.

---

# 36. Validation Pipeline

Before an article becomes playable:

```text
validate source
validate narration
validate audio
validate timestamps
validate manifest
validate presentation components
```

Checks should include:

- no missing referenced blocks,
- no invalid timestamps,
- no overlapping impossible states,
- all audio segments exist,
- all presentation components are valid,
- manifest schema is valid.

---

# 37. Performance Requirements

## Playback

Playback must remain smooth without network access once the artifact is generated.

Target:

- stable 60 FPS UI where practical,
- low-latency seeking,
- no agent dependency during playback.

## Generation

Generation time may be long because it is an offline pipeline.

The UI must therefore expose progress.

Example:

```text
Generating presentation

██████████████░░░░░░ 68%

Generating visual explanation
```

---

# 38. MVP

The MVP should intentionally be narrow.

## MVP Goal

Prove that:

> An article can be converted into a significantly better consumption experience through synchronized narration and text.

### MVP Features

- Tauri desktop application.
- Paste one URL.
- Article extraction.
- Normalized article representation.
- Agent-driven narration planning.
- One configurable TTS provider.
- Audio generation.
- Sentence-level timestamps.
- Synchronized text highlighting.
- Automatic scrolling.
- Play/pause/seek.
- Playback speed.
- Article history.
- Local artifact storage.
- Manifest-based playback.

### Explicitly Excluded from MVP

- complex generated diagrams,
- animated visualizations,
- personalization,
- knowledge graph,
- cloud synchronization,
- multi-user accounts,
- mobile application,
- social features.

---

# 39. V1

After MVP validation:

- word-level highlighting,
- section navigation,
- multiple audio providers,
- local TTS support,
- richer article extraction,
- code block synchronization,
- concept cards,
- bookmarks,
- resume playback,
- generation retry by stage.

---

# 40. V2

Introduce the presentation engine.

Features:

- diagrams,
- charts,
- architecture visualizations,
- timelines,
- code walkthroughs,
- animated processes,
- visual transitions,
- presentation planner.

The core addition is:

```text
Article
 ↓
Narration
 +
Presentation Plan
 ↓
Interactive Lesson
```

---

# 41. V3

Introduce adaptive learning.

Features:

- user knowledge profile,
- concept graph,
- personalized narration,
- adaptive explanations,
- related article recommendations,
- semantic search,
- review sessions,
- concept quizzes.

---

# 42. Non-Goals

The product is not initially intended to be:

- a general-purpose web browser,
- a podcast hosting platform,
- a social network,
- a generic AI chatbot,
- a video editor,
- a replacement for original articles,
- a fully autonomous research agent.

The original source should remain accessible.

---

# 43. Success Metrics

## Primary Metric

**Completion rate of long-form articles.**

Compare:

```text
normal reading completion
vs
generated presentation completion
```

## Secondary Metrics

- average article completion percentage,
- number of sessions per article,
- average uninterrupted listening duration,
- percentage of generated articles actually played,
- number of articles completed per week,
- number of rewinds/replays,
- number of generated visualizations viewed,
- time from URL submission to playable artifact.

## Quality Metrics

- narration quality,
- synchronization accuracy,
- source fidelity,
- visualization usefulness,
- extraction accuracy,
- failure rate.

---

# 44. Quality Bar

A generated presentation should feel:

- accurate,
- calm,
- focused,
- technically competent,
- visually coherent,
- synchronized,
- useful rather than gimmicky.

A visualization should answer:

> "Does this make the concept easier to understand?"

If not, omit it.

---

# 45. Example End-to-End Flow

User pastes:

```text
https://example.com/article-about-raft
```

The system:

### Step 1

Fetches the page.

### Step 2

Extracts:

```text
Title
Sections
Paragraphs
Code
Images
References
```

### Step 3

Agent analyzes:

```text
Concepts:
- leader election
- log replication
- quorum
- term
- commit index
```

### Step 4

Agent creates narration.

### Step 5

TTS generates audio.

### Step 6

Alignment produces word timestamps.

### Step 7

Agent identifies:

```text
Leader election → visualization
Replication → animation
Quorum → diagram
```

### Step 8

Presentation components are generated.

### Step 9

Manifest is assembled.

### Step 10

Tauri loads:

```text
audio
+
article
+
visual timeline
+
presentation manifest
```

### Step 11

The user presses play.

The experience becomes:

```text
Narration:
"Raft begins by dividing time into terms..."

UI:
        Term 1
   ┌─────────────┐
   │   Follower  │
   └─────────────┘

Narration:
"When a follower stops hearing..."

UI:
        Term 2

      Candidate
          ↓
    Request Vote
       ↙   ↘
   Node A   Node B
```

Then it returns to the article.

---

# 46. Technical Architecture

```text
┌─────────────────────────────────────────────────────────┐
│                      TAURI APP                          │
│                                                         │
│  ┌──────────────────────┐   ┌────────────────────────┐  │
│  │      Web UI          │   │       Rust Core         │  │
│  │                      │   │                         │  │
│  │ Library              │   │ Job Manager             │  │
│  │ Presentation Canvas  │◄─►│ ACP Client              │  │
│  │ Player               │   │ Artifact Manager        │  │
│  │ Controls             │   │ Storage                 │  │
│  └──────────────────────┘   └────────────┬───────────┘  │
│                                          │              │
└──────────────────────────────────────────┼──────────────┘
                                           │ ACP
                                           ▼
                                ┌──────────────────────┐
                                │    AGENT HARNESS     │
                                │                      │
                                │ System Prompt        │
                                │ Skills               │
                                │ Tools                │
                                └──────────┬───────────┘
                                           │
                    ┌──────────────────────┼────────────────────┐
                    │                      │                    │
                    ▼                      ▼                    ▼
              Content Tools          Audio Tools          Visual Tools
                    │                      │                    │
                    ▼                      ▼                    ▼
              Fetch/Extract             TTS                 Diagrams
              Normalize                 Align               Charts
              Analyze                   Encode              Animations
```

---

# 47. Suggested Repository Structure

```text
article-engine/
├── apps/
│   └── desktop/
│       ├── src/
│       │   ├── components/
│       │   ├── features/
│       │   ├── player/
│       │   ├── presentation/
│       │   └── state/
│       └── src-tauri/
│           ├── src/
│           │   ├── commands/
│           │   ├── jobs/
│           │   ├── acp/
│           │   ├── storage/
│           │   ├── artifacts/
│           │   └── audio/
│           └── Cargo.toml
│
├── packages/
│   ├── manifest/
│   ├── presentation-schema/
│   ├── agent-protocol/
│   └── shared-types/
│
├── agent/
│   ├── system-prompt.md
│   ├── skills/
│   │   ├── ingestion/
│   │   ├── analysis/
│   │   ├── narration/
│   │   ├── visualization/
│   │   └── validation/
│   └── tools/
│
├── pipeline/
│   ├── ingestion/
│   ├── narration/
│   ├── audio/
│   ├── alignment/
│   └── presentation/
│
├── schemas/
│   ├── article.schema.json
│   ├── manifest.schema.json
│   └── presentation.schema.json
│
└── docs/
    ├── architecture.md
    ├── agent.md
    ├── manifest.md
    └── development.md
```

---

# 48. Key Interfaces

## Article

```text
Article
 ├── metadata
 ├── sections
 └── blocks
```

## Narration

```text
Narration
 ├── segments
 ├── text
 └── timing
```

## Presentation

```text
Presentation
 ├── segments
 ├── components
 └── timeline
```

## Artifact

```text
Artifact
 ├── source
 ├── audio
 ├── alignment
 ├── presentation
 └── metadata
```

---

# 49. Development Roadmap

## Phase 0 — Architecture

Deliver:

- repository,
- Tauri shell,
- Rust core,
- React UI,
- SQLite,
- artifact store,
- manifest schema,
- ACP abstraction.

Exit criteria:

- application launches,
- ACP session can be established,
- local artifact can be created and loaded.

---

## Phase 1 — Article Pipeline

Deliver:

- URL input,
- fetching,
- extraction,
- normalization,
- article storage.

Exit criteria:

- representative articles produce clean normalized documents.

---

## Phase 2 — Narration

Deliver:

- narration planning,
- TTS provider,
- audio artifacts,
- playback.

Exit criteria:

- an article can be played as audio.

---

## Phase 3 — Synchronization

Deliver:

- sentence timing,
- automatic scrolling,
- highlighted text,
- seeking.

Exit criteria:

- audio and article remain synchronized during normal playback and seeking.

---

## Phase 4 — Presentation Runtime

Deliver:

- manifest-driven rendering,
- component registry,
- segment transitions,
- code presentation,
- basic cards.

Exit criteria:

- playback can deterministically render multiple presentation types.

---

## Phase 5 — AI Visualizations

Deliver:

- visualization planning,
- diagram components,
- charts,
- animations,
- code walkthroughs.

Exit criteria:

- the agent can choose an appropriate visual representation for a concept and produce a valid manifest.

---

## Phase 6 — Personal Learning Layer

Deliver:

- concept graph,
- user preferences,
- semantic search,
- personalized explanations,
- review.

Exit criteria:

- repeated usage results in meaningfully personalized presentations.

---

# 50. Testing Strategy

## Unit Tests

Test:

- article parsing,
- manifest validation,
- timestamp calculations,
- timeline resolution,
- storage,
- cache keys,
- presentation component validation.

## Integration Tests

Test:

```text
URL
 ↓
normalized article
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

## Golden Tests

Maintain known articles and expected:

- normalized structures,
- narration segment boundaries,
- manifest schemas.

## Playback Tests

Verify:

- play,
- pause,
- seek,
- resume,
- speed changes,
- section navigation,
- synchronization.

## Agent Tests

Use fixture articles to evaluate:

- source fidelity,
- visualization decisions,
- manifest validity,
- unnecessary hallucination,
- explanation quality.

---

# 51. Failure Philosophy

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

Agent failure
        ↓
Resume from last valid artifact
```

The core reading experience must not depend on optional intelligence.

---

# 52. Future Advanced Capabilities

Potential future capabilities include:

## Adaptive Narration

The system changes explanation depth based on user interaction.

## Interactive Questions

The presentation pauses and asks:

> "Why does this node need a quorum?"

## Concept Exploration

User clicks a concept and gets a short visual explanation without leaving the presentation.

## Generated Examples

The agent creates a minimal runnable example.

## Live Code Execution

Safe sandboxed code execution for technical articles.

## Multi-Article Synthesis

Combine several articles into one narrated lesson.

## Personal Curriculum

Automatically turn consumed material into a structured learning path.

## Article-to-Course

Transform a collection of articles into chapters and lessons.

---

# 53. Risks

## Risk: Hallucinated explanations

Mitigation:

- source mapping,
- explicit generated-content labels,
- claim provenance,
- validation.

## Risk: Bad visualizations

Mitigation:

- only generate when useful,
- component-based rendering,
- agent validation,
- human-readable fallback.

## Risk: TTS quality

Mitigation:

- provider abstraction,
- local/cloud options,
- voice testing,
- pronunciation metadata.

## Risk: Synchronization drift

Mitigation:

- segment-level audio,
- alignment pipeline,
- deterministic manifest,
- validation.

## Risk: Agent dependency

Mitigation:

- ACP abstraction,
- deterministic runtime,
- persisted intermediate artifacts.

## Risk: Generation cost/time

Mitigation:

- caching,
- incremental generation,
- segment-level retries,
- configurable presentation depth.

---

# 54. MVP Definition of Done

The MVP is complete when a user can:

1. Open the Tauri application.
2. Paste an article URL.
3. Start generation.
4. Watch generation progress.
5. Wait for processing to complete.
6. Open the generated article.
7. Press play.
8. Hear a coherent narration.
9. See the relevant text automatically highlighted.
10. See the article scroll with the narration.
11. Pause and resume.
12. Seek to another point.
13. Change playback speed.
14. Close and reopen the application.
15. Resume the article from the previous position.

The entire generated presentation must work without an active agent session.

---

# 55. North Star

The north-star experience is:

> **Paste an article. Press play. Learn.**

The system should make a long technical article feel like a carefully produced educational video without requiring the user to manually:

- read everything,
- research every concept,
- find diagrams,
- look up code,
- create notes,
- search for explanations,
- or produce their own audio.

The generated artifact should preserve the original article while adding a second layer of intelligence:

```text
                 SOURCE
                   │
                   ▼
              ARTICLE
                   │
          ┌────────┴────────┐
          ▼                 ▼
       NARRATION         PRESENTATION
          │                 │
          │          ┌──────┼──────┐
          │          ▼      ▼      ▼
          │        TEXT   CODE   VISUALS
          │                 │      │
          └────────┬────────┴──────┘
                   ▼
              TIMELINE
                   │
                   ▼
          INTERACTIVE LESSON
```

The central architectural idea is therefore:

> **The agent compiles source material into a synchronized multimedia presentation manifest, and the Tauri runtime deterministically executes that presentation.**

This boundary should remain stable even as models, agent harnesses, TTS systems, and visualization capabilities evolve.
