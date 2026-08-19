# Digest — Shared Contracts (TS ↔ Rust)

> The typed seam between the Rust core and the React frontend. This document is the **agreement** — all code implements against it and keeps it in sync.
> See [Decision D-010](decisions.md#d-010--contracts-manually-authored-types) for why we hand-author types instead of generating them.

---

## The Rule

**A contract is a promise across the IPC seam.** Any change to a contract must land in all three places in the same change:

1. This document ([contracts.md](contracts.md))
2. The Rust type under `src-tauri/src/contracts/`
3. The TypeScript type under `src/types/contracts/`

No silent one-sided changes. If a contract breaks, the failing side is the one that did not follow the rule.

---

## Where the types live

| Side | Location |
| :--- | :--- |
| **Rust Core** | `src-tauri/src/contracts/` — `pub struct` / `enum` with `serde::{Serialize, Deserialize}` |
| **TypeScript UI** | `src/types/contracts/` — interfaces and union types mirroring the Rust structs |
| **Mock Fixtures** | `src/mocks/` in the frontend for offline UI testing |

---

## The Contracts

---

### `ipc.ts` — Core IPC Commands, Results & Errors

Carries typed command results, query responses, and pipeline errors crossing the Tauri bridge.

```ts
export type AppResult<T> = { ok: true; data: T } | { ok: false; error: AppError };

export type AppError =
  | { kind: "io"; message: string }
  | { kind: "database"; message: string }
  | { kind: "extractor"; message: string }
  | { kind: "acp"; message: string }
  | { kind: "audio"; message: string }
  | { kind: "alignment"; message: string }
  | { kind: "validation"; message: string }
  | { kind: "notFound"; message: string }
  | { kind: "internal"; message: string };

export interface PingResponse {
  message: string;
  version: string;
  timestamp: number;
}
```

---

### `article.ts` — Normalized Article Representation

Carries the clean, structural representation of an ingested article.

```ts
export interface NormalizedArticle {
  id: string;
  sourceUrl: string;
  title: string;
  author: string | null;
  publishedAt: string | null;
  summary: string | null;
  sections: ArticleSection[];
  totalWordCount: number;
  estimatedReadTimeMinutes: number;
  extractedAt: number;
}

export interface ArticleSection {
  id: string;
  heading: string | null;
  level: number;
  blocks: ArticleBlock[];
}

export type ArticleBlock =
  | { id: string; type: "paragraph"; text: string }
  | { id: string; type: "heading"; text: string; level: number }
  | { id: string; type: "code"; text: string; language: string; caption?: string }
  | { id: string; type: "quote"; text: string; citation?: string }
  | { id: string; type: "list"; items: string[]; ordered: boolean }
  | { id: string; type: "callout"; text: string; tone: "info" | "warning" | "tip" | "important" }
  | { id: string; type: "image"; url: string; altText?: string; caption?: string }
  | { id: string; type: "table"; headers: string[]; rows: string[][] };
```

---

### `concepts.ts` — Semantic Concept Graph & Analysis

Carries extracted technical concepts, difficulty scores, claims, and visualization opportunities produced by the agent.

```ts
export interface ConceptAnalysis {
  articleId: string;
  concepts: ConceptItem[];
  keyClaims: KeyClaim[];
  visualizationOpportunities: VisualizationOpportunity[];
  prerequisites: string[];
}

export interface ConceptItem {
  id: string;
  name: string;
  category: "theory" | "architecture" | "algorithm" | "syntax" | "business" | "general";
  difficulty: "beginner" | "intermediate" | "advanced";
  definition: string;
  sourceBlockIds: string[];
  suggestedVisualComponent: string | null;
}

export interface KeyClaim {
  id: string;
  claim: string;
  sourceBlockIds: string[];
  isQuantitative: boolean;
  dataPoints?: Record<string, number | string>;
}

export interface VisualizationOpportunity {
  id: string;
  conceptId: string;
  targetType: "diagram" | "code-walkthrough" | "chart" | "animation" | "concept-card";
  rationale: string;
  sourceBlockIds: string[];
}
```

---

### `narration.ts` — Narration Scripts & Segments

Carries the generated spoken script, broken into timeable segments with explicit source attribution.

```ts
export type NarrationMode = "faithful" | "explained" | "deep-dive" | "executive";

export interface NarrationPlan {
  articleId: string;
  mode: NarrationMode;
  segments: NarrationSegment[];
  totalEstimatedDurationSeconds: number;
}

export interface NarrationSegment {
  id: string;
  index: number;
  sectionId: string;
  sourceBlockIds: string[];
  spokenText: string;
  origin: "source" | "explanation" | "demonstration";
  visualCue: string | null;
  pronunciationNotes?: Record<string, string>;
}
```

---

### `audio.ts` — Audio Synthesis & Providers

Carries audio generation requests, cached audio artifact references, and TTS provider configurations.

```ts
export interface AudioArtifact {
  segmentId: string;
  filePath: string;
  durationSeconds: number;
  sampleRate: number;
  channels: number;
  format: "wav" | "opus" | "aac";
  fileSizeBytes: number;
  sha256: string;
}

export interface MasterAudioTrack {
  articleId: string;
  filePath: string;
  totalDurationSeconds: number;
  format: "opus";
  fileSizeBytes: number;
}

export interface TTSProviderConfig {
  provider: "piper" | "kokoro" | "openai" | "elevenlabs" | "custom";
  voiceId: string;
  speed: number;
  apiEndpoint?: string;
}
```

---

### `alignment.ts` — Multi-Tier Timestamp Alignment

Carries paragraph, sentence, and word-level audio alignment data.

```ts
export type AlignmentPrecision = "word" | "sentence" | "paragraph";

export interface AlignmentReport {
  articleId: string;
  precision: AlignmentPrecision;
  segments: SegmentTiming[];
}

export interface SegmentTiming {
  segmentId: string;
  start: number; // Seconds
  end: number;
  sentences: SentenceTiming[];
}

export interface SentenceTiming {
  text: string;
  start: number;
  end: number;
  words?: WordTiming[];
}

export interface WordTiming {
  word: string;
  start: number;
  end: number;
  confidence?: number;
}
```

---

### `presentation.ts` — Structured Visual Components

Carries declarative visual widgets rendered by the React presentation canvas.

```ts
export type PresentationComponent =
  | ArticleTextComponent
  | DiagramComponent
  | CodeWalkthroughComponent
  | ChartComponent
  | ConceptCardComponent
  | AnimatedFlowComponent;

export interface ArticleTextComponent {
  type: "article-text";
  blockIds: string[];
  highlightMode: "word" | "sentence" | "paragraph";
}

export interface DiagramComponent {
  type: "diagram";
  engine: "mermaid" | "svg-graph";
  source: string;
  activeNodeId?: string;
  caption?: string;
}

export interface CodeWalkthroughComponent {
  type: "code-walkthrough";
  language: string;
  code: string;
  activeLineRange?: [number, number];
  annotations?: Array<{ line: number; text: string }>;
}

export interface ChartComponent {
  type: "chart";
  chartType: "bar" | "line" | "pie";
  title: string;
  xAxisLabel?: string;
  yAxisLabel?: string;
  data: Array<{ label: string; value: number }>;
}

export interface ConceptCardComponent {
  type: "concept-card";
  title: string;
  badge: string;
  summary: string;
  bulletPoints: string[];
  origin: "source" | "explanation";
}

export interface AnimatedFlowComponent {
  type: "animated-flow";
  steps: Array<{
    stepNumber: number;
    title: string;
    description: string;
    diagramState: string;
  }>;
  activeStep: number;
}
```

---

### `lesson.ts` — Compiled Lesson & Timeline

Carries the full compiled presentation model queried from SQLite by the desktop player.

```ts
export interface Lesson {
  id: string;
  articleId: string;
  title: string;
  sourceUrl: string;
  author: string | null;
  summary: string | null;
  audioTrack: {
    durationSeconds: number;
    filePath: string;
    format: "opus" | "wav";
  };
  timeline: TimelineSegment[];
  narrationMode: NarrationMode;
  alignmentPrecision: AlignmentPrecision;
  generatedAt: number;
}

export interface TimelineSegment {
  id: string;
  index: number;
  start: number; // Seconds
  end: number;
  sourceBlockIds: string[];
  narration: {
    text: string;
    origin: "source" | "explanation" | "demonstration";
    words?: Array<{ word: string; start: number; end: number }>;
    sentences?: Array<{ text: string; start: number; end: number }>;
  };
  presentation: PresentationComponent;
}
```

---

### `acp.ts` — Agent Client Protocol (ACP) Seam

Carries communication between the Rust core and external agent harnesses.

```ts
export interface SessionRequest {
  articleId: string;
  articleUrl: string;
  userPrompt?: string;
  mode?: string;
}

export interface AgentSession {
  sessionId: string;
  articleId: string;
  status: "idle" | "running" | "completed" | "failed";
  createdAt: number;
}

export interface SkillInvocation {
  skill: "ingest" | "analyze" | "script" | "visualize" | "validate";
  payload: Record<string, unknown>;
}
```

---

### `jobs.ts` — Durable Job State & Pipeline Events

Carries generation job lifecycle events and real-time progress broadcasts to the webview.

```ts
export type JobStage =
  | "queued"
  | "fetching"
  | "extracting"
  | "analyzing"
  | "scripting"
  | "synthesizing_audio"
  | "aligning"
  | "planning_visuals"
  | "assembling_timeline"
  | "validating"
  | "ready"
  | "failed";

export interface GenerationJob {
  id: string;
  articleId: string;
  stage: JobStage;
  progressPercent: number;
  statusMessage: string;
  createdAt: number;
  updatedAt: number;
  error?: string;
}

export interface JobProgressEvent {
  jobId: string;
  articleId: string;
  stage: JobStage;
  progressPercent: number;
  statusMessage: string;
}
```

---

## Changing a contract

1. **Agree on the shape.** Prefer backwards-compatible changes (add optional fields) over breaking existing structures.
2. **Land in one change:** Update this document + the Rust structs (`src-tauri/src/contracts/`) + the TypeScript interfaces (`src/types/contracts/`).
3. **Update mock fixtures** in `src/mocks/` to match the new shape.
