# Digest — Shared Contracts (TS ↔ Rust)

> The typed seam between the Rust core and the React frontend. This document is the **agreement** — all code implements against it and keeps it in sync.
> It describes the code as it exists at commit `3d99c32`, not as originally planned. There is no `src-tauri/src/contracts/` and no `src/types/contracts/`: the Rust structs live beside the code that owns them, and the TypeScript types are declared at the top of `src/App.tsx`.
> See [Decision D-010](decisions.md#d-010--contracts-manually-authored-types) for why we hand-author types instead of generating them.

---

## The Rule

**A contract is a promise across the IPC seam.** Any change to a contract must land in all three places in the same change:

1. This document ([contracts.md](contracts.md))
2. The Rust type, in the module that owns it under `src-tauri/src/`
3. The TypeScript type, in `src/App.tsx`

No silent one-sided changes. If a contract breaks, the failing side is the one that did not follow the rule.

---

## Where the types live

| Side | Location |
| :--- | :--- |
| **Rust Core** | `src-tauri/src/` — `pub struct` / `enum` beside the owning module, with `serde::{Serialize, Deserialize}` and `schemars::JsonSchema` where the type is an MCP tool argument |
| **TypeScript UI** | `src/App.tsx` — the type declarations at the top of the file (`Artifact`, `RunSnapshot`, `HostInfo`, `PlaybackManifest`, `PlaybackSegment`, `PlaybackPart`, …) |
| **Schema derivation** | `schemars` derives the JSON Schema for each MCP tool argument from the Rust type. There is no separate `.json` schema file and no generation step. |

There are no mock fixtures. The frontend has no standalone browser mode; `pnpm dev` serves the app but every command it calls is a real Tauri command, so the UI only runs inside the desktop shell (`pnpm tauri dev`).

---

## The Contracts

---

### Tauri command surface

The authoritative list is the `tauri::generate_handler!` invocation in `src-tauri/src/lib.rs:41`. Every handler is a `#[tauri::command]` in `src-tauri/src/host.rs`.

| Command | Handler | Arguments | Returns |
| :--- | :--- | :--- | :--- |
| `host_info` | `host::host_info` | none | `HostInfo` |
| `run_snapshot` | `host::run_snapshot` | `jobId: string`, `afterEventSequence?: number` | `RunSnapshot` |
| `recent_runs` | `host::recent_runs` | `limit?: number` (defaults to 20) | `RunSummary[]` |
| `start_agent_run` | `host::start_agent_run` | `input: StartAgentRun` | `AgentRunResult` |
| `list_agent_models` | `host::list_agent_models` | `input: { provider, cwd }` | `AgentModel[]` |
| `cancel_agent_run` | `host::cancel_agent_run` | `jobId: string` | `void` |
| `generate_audio` | `host::generate_audio` | `input: GenerateAudio`, `onProgress: Channel<AudioGenerationProgress>` | `GenerateAudioResult` |
| `regenerate_segment_audio` | `host::regenerate_segment_audio` | `input: RegenerateSegmentAudio`, `onProgress: Channel<AudioGenerationProgress>` | `GenerateAudioResult` |
| `cancel_audio_generation` | `host::cancel_audio_generation` | `jobId: string` | `void` |
| `audio_asset` | `host::audio_asset` | `artifactId: string` | raw binary body (`tauri::ipc::Response`) |

`start_agent_run`, `generate_audio`, and `regenerate_segment_audio` are `async`. `run_snapshot` returns a full snapshot when `afterEventSequence` is omitted and an incremental projection after it, which is how the UI avoids re-transferring a multi-megabyte agent history on every poll.

`audio_asset` returns raw bytes, not JSON. The frontend wraps them in a `Blob` using the MIME type carried by the playback manifest, not one derived from a file extension.

Failure is reported as a rejected promise carrying `Err(String)` — the handler's `error.to_string()`. There is no structured `AppError` union on the wire. The typed error enums (`DigestError`, `IngestionError`, `AudioError`, `AcpClientError`) are flattened to a string at the command boundary.

---

### `application.rs` — Artifact envelope, attempts, and events

Every durable thing the pipeline produces is an `ArtifactEnvelope` row in the `artifacts` table. Article content, narration plans, audio bytes, and playback manifests all share this one envelope; there is no separate `articles` or `lessons` table.

```ts
type Artifact = {
  schemaVersion: string;   // "1.0" for every envelope written by DigestService
  artifactId: string;
  jobId: string;
  kind: string;            // ArtifactKind, snake_case
  contentHash: string;     // SHA-256 of the payload bytes
  createdAtMs: number;
  payload: Record<string, unknown>;
};
```

`kind` is one of:

| Rust `ArtifactKind` | Wire value | Payload |
| :--- | :--- | :--- |
| `Analysis` | `analysis` | the agent's analysis JSON |
| `SourceCapture` | `source_capture` | immutable raw response metadata |
| `NormalizedArticle` | `normalized_article` | `NormalizedArticle` (see below) |
| `ImageAsset` | `image_asset` | image metadata; the bytes live in the object store |
| `NarrationPlan` | `narration_plan` | the narration plan JSON |
| `AudioSegment` | `audio_segment` | audio segment metadata; the bytes live in the object store |
| `PlaybackManifest` | `playback_manifest` | the manifest (see below) |

`schemaVersion` on the envelope is the *envelope* version, fixed at `"1.0"` by `ARTIFACT_SCHEMA_VERSION` in `src-tauri/src/application.rs:15`. It is not the version of the payload inside. Payloads carry their own `schemaVersion`: the normalized article is `"1.3"`, an audio segment is `"1.1"`, a narration plan is `"1.5"`, and a playback manifest is `"1.4"`.

```ts
type RunAttempt = {
  attemptId: string;
  jobId: string;
  provider: string;               // "open_code" | "agy"
  providerSessionId: string | null;
  status: "running" | "completed" | "failed" | "cancelled";
  startedAtMs: number;
  finishedAtMs: number | null;
  error: string | null;
};

type RunSummary = {
  jobId: string;
  title: string | null;
  provider: string | null;
  status: "captured" | "running" | "completed" | "failed" | "cancelled";
  updatedAtMs: number;
  artifactCount: number;
  eventCount: number;
};

type RunSnapshot = {
  attempts: RunAttempt[];
  events: AgentEvent[];
  artifacts: Artifact[];
};
```

`AgentEvent.kind` is the `NewAgentEventKind` set, serialized snake_case: `session_started`, `agent_message`, `agent_thinking`, `tool_started`, `tool_progress`, `tool_completed`, `tool_failed`, `artifact_created`, `session_completed`, `session_cancelled`, `session_failed`, `session_reminded`.

Attempts end `completed` only when the agent produced both the analysis and the narration plan. When the session ends with required work still missing — after up to two follow-up prompts naming the exact outstanding steps — the attempt is `incomplete`, never `completed`, with the missing artifacts named in its error. `incomplete` flows through to `RunStatus` the same way.

`HostInfo` reports the resolved runtime environment to the UI:

```ts
type HostInfo = {
  dataDir: string;
  mcpExecutable: string;
  agentWorkspaceDir: string;               // <dataDir>/workspaces/default, created at startup
  agentInactivityTimeoutSeconds: number;
  agentMaxRuntimeSeconds: number | null;   // null when no limit is configured
  kokoroEndpoint: string;
  audioProvider: string;
  audioDefaultVoice: string;
};
```

`agentWorkspaceDir` is the default ACP working directory the UI offers for a new run. The host guarantees the directory exists. It sits under the data directory so it is absolute and writable, and outside the source tree so an agent run cannot touch the repository.

---

### `ingestion.rs` — Normalized article

The normalized article is a flat ordered block list, not a section tree. A block is an internally tagged enum on the `kind` field.

```ts
type NormalizedArticle = {
  schemaVersion: string;        // "1.3"
  canonicalUrl: string;
  title: string | null;
  blocks: ArticleBlock[];
  images: ArticleImage[];
  diagnostics: ExtractionDiagnostics;
};

type ArticleBlock =
  | { kind: "heading"; id: string; level: number; text: string; imageIds: string[] }
  | { kind: "paragraph"; id: string; text: string; imageIds: string[] }
  | { kind: "code"; id: string; language: string | null; text: string; imageIds: string[] }
  | { kind: "list"; id: string; ordered: boolean; items: string[]; imageIds: string[] }
  | { kind: "quote"; id: string; text: string; imageIds: string[] }
  | { kind: "diagram"; id: string; text: string; imageIds: string[] };

`imageIds` links each article image to the block it illustrates, in document order: an image attaches to the nearest preceding kept block, and images before the first block attach to it. Images after the trailing boilerplate boundary are post-content chrome and never surface. Old articles without the key read as imageless, which is exactly what their plans assumed.

type ArticleImage = {
  id: string;
  originalUrl: string;
  sourceUrl: string;
  alt: string | null;
  title: string | null;
  caption: string | null;
  width: number | null;
  height: number | null;
  srcsetCandidates: string[];
  captureStatus: "pending" | "localized" | "failed";
  artifactId: string | null;     // set only when captureStatus is "localized"
  mimeType: string | null;       // detected from bytes, not from the extension
  contentHash: string | null;
  byteLength: number | null;
  error: string | null;          // recorded failure, never a silent omission
};

type ExtractionDiagnostics = {
  confidence: number;            // 0-255, u8
  wordCount: number;
  blockCount: number;
  imageCount: number;
  diagramCount: number;
  warnings: string[];
};
```

---

### The MCP tool seam

`tools.rs` and `mcp.rs`: the agent reaches Digest's deterministic services over MCP, not over ACP. ACP is the client-to-agent control channel; MCP is the agent-to-tool channel. `src-tauri/src/mcp.rs` registers four tools, each delegating to a transport-independent `DigestTools` method in `src-tauri/src/tools.rs`, which delegates to the same `DigestService` the Tauri commands use.

| MCP tool | Input | Output |
| :--- | :--- | :--- |
| `ingest_article` | `IngestArticleInput` | `IngestArticleOutput` |
| `read_artifact` | `ReadArtifactInput` | `ReadArtifactOutput` |
| `write_analysis` | `WriteAnalysisInput` | `WriteAnalysisOutput` |
| `write_narration_plan` | `WriteNarrationPlanInput` | `ArtifactEnvelope` |

```ts
// Analysis provenance. The kind is what separates a claim the source makes
// from a claim the model inferred.
type ProvenanceKind =
  | "source_derived"
  | "ai_explanation"
  | "ai_inference"
  | "generated_educational";

type AnalysisClaim = {
  text: string;
  sourceBlocks: string[];
  kind: ProvenanceKind;
};

type AnalysisFindingKind =
  | "major_concept" | "supporting_concept" | "key_claim" | "example"
  | "definition" | "comparison" | "causal_relationship" | "code_example"
  | "quantitative_claim" | "important_entity" | "difficult_section"
  | "prerequisite" | "visualization_opportunity" | "point_of_confusion";

type AnalysisFinding = {
  category: AnalysisFindingKind;
  text: string;
  sourceBlocks: string[];
  kind: ProvenanceKind;
};

type WriteAnalysisInput = {
  jobId: string;
  articleId: string;
  centralArgument: AnalysisClaim;
  findings: AnalysisFinding[];
};

type WriteAnalysisOutput = { artifactId: string; contentHash: string };
```

```ts
// Narration plan. displayText is what the reader sees; ttsText is what the
// provider speaks. They are validated against each other so ttsText may only
// normalize pronunciation, never add or drop content.
type PresentationType =
  | "article-text" | "callout" | "code" | "concept-card" | "diagram" | "image" | "quote";

type NarrationImportance = "core" | "supporting";

type NarrationIntent =
  | "introduction" | "explanation" | "example" | "comparison"
  | "quantification" | "takeaway";

type NarrationSegmentDraft = {
  displayText: string;
  ttsText: string;
  sourceBlocks: string[];
  presentationType: PresentationType;
  visual?: VisualSpec;              // required for diagram and concept-card, forbidden elsewhere
  imageId?: string;                 // required for image segments, forbidden elsewhere
  importance: NarrationImportance;
  intent: NarrationIntent;
  provenance: ProvenanceKind;
};

// Authored visual content (narration schema 1.4+, manifest schema 1.4+). The
// player draws exactly this; nothing is inferred from displayText. Segments
// from older plans carry no visual and fall back to the narration text.
type VisualSpec =
  | { type: "diagram"; nodes: { id: string; label: string }[]; edges: { from: string; to: string; label?: string }[] }
  | { type: "points"; items: string[] };

type SourceCoverageTreatment = "teach" | "summarize" | "skip";

type SourceCoverageDecision = {
  sourceBlocks: string[];
  treatment: SourceCoverageTreatment;
  rationale: string;
};

type WriteNarrationPlanInput = {
  jobId: string;
  articleId: string;
  title: string;
  segments: NarrationSegmentDraft[];
  sourceCoverageDecisions: SourceCoverageDecision[];
  imageCoverageDecisions: ImageCoverageDecision[];
};

type ImageCoverageTreatment = "present" | "skip";

type ImageCoverageDecision = {
  imageIds: string[];               // localized images only; decorative ones never surface
  treatment: ImageCoverageTreatment;
  rationale: string;
};
```

`PresentationType` is the only enum in the crate serialized kebab-case; everything else on this seam is snake_case for enums and camelCase for structs. Every source block and every diagram block must appear in exactly one `sourceCoverageDecision`, and every `teach` or `summarize` block must be cited by a segment. Every localized image must appear in exactly one `imageCoverageDecision`, and every `present` image must be shown by an `image` segment carrying its `imageId`; images that failed localization carry no bytes and need no decision. `write_narration_plan` rejects the plan otherwise and returns coverage, image, and compression diagnostics rather than silently accepting an incomplete account.

---

### `acp.rs` — The agent run seam

`AgentProvider` is an enum, not a trait. There is no `#[async_trait] AgentProvider` with `start_session`/`send_prompt`/`cancel`, and no capability negotiation structure: each variant carries the launch information needed to spawn it.

```ts
type AgentProvider =
  | { provider: "open_code" }
  | { provider: "agy"; adapterCommand: string; adapterArgs: string[] };

type PermissionPolicy = "deny" | "allow_once";

type AgentRunResult = { sessionId: string; stopReason: string };
```

```ts
// The argument to start_agent_run.
type StartAgentRun = {
  jobId: string;
  provider: AgentProvider;
  cwd: string;
  articleUrl: string;
  prompt: string;
  model: string | null;           // default null; null keeps the harness default
  allowOncePermissions: boolean;  // default false
  refreshSource: boolean;         // default false; false reuses the latest normalized article
};

// One selectable harness model, as advertised by the agent itself.
type AgentModel = {
  id: string;                     // the wire value, e.g. "opencode/muse-spark-1.3-contributor-free"
  name: string;                   // display text
  description: string | null;
  current: boolean;               // the harness default for a fresh session
};
```

`host::start_agent_run` prepends the compilation prompt — the job ID, the normalized article's artifact ID, and the analysis/narration/coverage instructions — to `StartAgentRun.prompt` before handing it to `AcpClient`. The frontend only supplies the operator's own prompt text.

A requested `model` is applied through `session/set_config_option` after `session/new`, validated against the model selectors the harness advertised in its session config options. An unknown value fails the run with every accepted value named; a harness that advertises no model selector fails with instructions to run without one. Model selection is only supported for the OpenCode provider. `list_agent_models` spawns the harness, runs `initialize` plus `session/new`, and returns the advertised models without starting a run, so the UI picker only ever offers values the harness accepts.

`AgentRunSupervision` carries `inactivity_timeout` and an optional `max_runtime`; both are configurable through `DIGEST_AGENT_INACTIVITY_TIMEOUT_SECS` and `DIGEST_AGENT_MAX_RUNTIME_SECS`, and `max_runtime` is disabled by default. Cancellation is cooperative: `cancel_agent_run` flips a flag that the ACP read loop observes.

---

### `audio.rs` — The audio generation seam

`AudioProvider` is a trait, so a second provider only has to supply a name, a durable MIME type, a default voice, and a synthesis future:

```rust
pub trait AudioProvider: Send + Sync {
    fn name(&self) -> &'static str;
    /// MIME type of the durable audio container this provider delivers. The
    /// generation service refuses formats it cannot independently time.
    fn mime_type(&self) -> &'static str;
    fn default_voice(&self) -> &'static str;
    fn synthesize<'a>(
        &'a self,
        request: AudioRequest,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, AudioError>> + Send + 'a>>;
}
```

`KokoroProvider` is the only implementation. It posts to `{endpoint}/v1/audio/speech` with `response_format: "opus"` and `stream: false`, reports `mime_type() == "audio/ogg"`, and defaults to voice `af_sky`. The endpoint is `DIGEST_KOKORO_URL`, defaulting to `http://127.0.0.1:3000`.

The provider returns bytes. Digest decides what to trust about them, and never trusts the provider:

```rust
pub fn durable_audio_duration_ms(mime_type: &str, bytes: &[u8]) -> Result<u64, AudioError>;
```

| MIME type | How the duration is derived |
| :--- | :--- |
| `audio/wav` | `fmt ` byte rate against the `data` chunk length, treating a declared length of `0xFFFFFFFF` as "to end of file" |
| `audio/ogg`, `audio/opus` | RFC 7845: the last page's granule position minus the `OpusHead` pre-skip, in 48 kHz samples regardless of the encoder's input rate |
| anything else | rejected with `AudioError::UnsupportedFormat` before anything is persisted |

The Ogg reader also requires a single logical stream, an explicit end-of-stream page, and no data after it, so a truncated download cannot silently shorten a lesson.

Generation reports progress over the Tauri IPC channel at segment boundaries:

```ts
type AudioProgress = {
  totalSegments: number;
  completedSegments: number;
  generatedSegmentCount: number;
  reusedSegmentCount: number;
  currentSegmentId: string | null;   // null between segments
};

type GenerateAudioResult = {
  manifest: Artifact;
  generatedSegmentCount: number;
  reusedSegmentCount: number;
};
```

---

### Playback manifest schema `1.2`

`PLAYBACK_MANIFEST_SCHEMA_VERSION` in `src-tauri/src/audio.rs:15` is `"1.2"`. A segment carries an array of sentence parts; the segment's own `startMs`/`endMs` span the aggregate bounds of those parts, so the parts are contiguous inside the segment and the segments are contiguous inside the lesson.

```ts
type PlaybackManifest = {
  schemaVersion: string;
  articleId: string;
  title: string;
  audio: {
    provider: string;
    voice: string;
    speed: number;
    durationMs: number;      // sum of all part durations
  };
  segments: PlaybackSegment[];
};

type PlaybackSegment = {
  id: string;
  startMs: number;           // aggregate bounds of this segment's parts
  endMs: number;
  parts: PlaybackPart[];     // sentence-boundary transport parts, in playback order
  displayText: string;
  sourceBlocks: string[];
  provenance: unknown;       // pass-through from the narration plan
  presentation: unknown;     // pass-through from the narration plan
};

type PlaybackPart = {
  startMs: number;
  endMs: number;
  mimeType: string;          // "audio/ogg" today; per-part, not per-lesson
  audioArtifactId: string;   // an audio_segment artifact to fetch with audio_asset
  text: string;              // the sentence this part speaks
};
```

A minimal `1.2` manifest, two sentences inside one segment:

```json
{
  "schemaVersion": "1.2",
  "articleId": "article-1",
  "title": "Test article",
  "audio": {
    "provider": "kokoro",
    "voice": "af_sky",
    "speed": 1.0,
    "durationMs": 1500
  },
  "segments": [
    {
      "id": "s-1",
      "startMs": 0,
      "endMs": 1500,
      "parts": [
        {
          "startMs": 0,
          "endMs": 1000,
          "mimeType": "audio/ogg",
          "audioArtifactId": "art-1",
          "text": "This first sentence is comfortably longer than the forty character minimum."
        },
        {
          "startMs": 1000,
          "endMs": 1500,
          "mimeType": "audio/ogg",
          "audioArtifactId": "art-2",
          "text": "The second sentence also exceeds the configured minimum length easily."
        }
      ],
      "displayText": "Display for s-1",
      "sourceBlocks": ["block-1"],
      "provenance": { "kind": "source" },
      "presentation": { "type": "article-text" }
    }
  ]
}
```

`provenance` and `presentation` are opaque `serde_json::Value` on the Rust side and are copied from the narration plan without interpretation. The generator does not validate them; `write_narration_plan` does.

`1.3` kept the `1.2` shape and added authored visuals inside `presentation`; it never shipped, so `1.4` is the next released manifest and carries both additions. A `diagram` segment carries `{"type": "diagram", "visual": {"type": "diagram", "nodes": [...], "edges": [...]}}` and a `concept-card` segment carries `{"type": "concept-card", "visual": {"type": "points", "items": [...]}}`, both written by `write_narration_plan` under narration schema `1.5`. An `image` segment carries `{"image": {"imageId", "artifactId", "mimeType", "alt", "caption", "width", "height"}}` alongside `"presentation": {"type": "image"}`; the player fetches the bytes through the binary artifact endpoint and the key is omitted on every other segment. Segments from older plans carry neither key and fall back to the narration text, so every stored lesson keeps playing. The flat clip-list normalization is unchanged: `1.4` needs no new reader shape.

The `1.2` shape exists because of how the audio is stored. `split_into_sentence_parts` in `src-tauri/src/audio.rs:488` splits a segment's `ttsText` at sentence boundaries — a sentence ends at `.`, `!`, or `?` plus any trailing closing quote or bracket, followed by whitespace — and each part becomes its own `audio_segment` artifact. Parts shorter than 40 characters merge forward, and a short trailing fragment merges backward, so abbreviations like `E.g.` never become a degenerate synthesis request. Concatenating the parts reproduces the complete `ttsText`: this is transport subdivision, never a content limit.

### Playback manifest schemas `1.0` and `1.1`

Before `1.2`, a segment held a single artifact for the whole segment:

```ts
type PlaybackSegment = {
  id: string;
  startMs: number;
  endMs: number;
  parts?: PlaybackPart[];        // 1.2+
  mimeType?: string;             // 1.0/1.1 only
  audioArtifactId?: string;      // 1.0/1.1 only
  displayText: string;
  sourceBlocks: string[];
  presentation: { type?: string };
};
```

`parts`, `mimeType`, and `audioArtifactId` are all optional in the frontend type because a single reader must handle all three schema versions against artifacts that are already durable. On a `1.2` manifest `mimeType` and `audioArtifactId` are **absent** at segment level, and on a `1.0`/`1.1` manifest `parts` is **absent**. Code that reads the playback model must not assume either field is present.

### Frontend normalization into one clip list

The player never iterates segments directly. `src/App.tsx:438` flattens the manifest into a single `PlaybackClip[]` — one playable clip per sentence part — so the transport has exactly one notion of "the thing currently loaded into the audio element":

```ts
const clips = useMemo<PlaybackClip[]>(
  () =>
    manifest.segments.flatMap((segment, segmentIndex) =>
      (segment.parts && segment.parts.length > 0
        ? segment.parts
        : [{ startMs: segment.startMs, endMs: segment.endMs,
             mimeType: segment.mimeType, audioArtifactId: segment.audioArtifactId ?? "" }]
      ).map((part) => ({ ...part, segmentIndex })),
    ),
  [manifest],
);
```

A `1.2` segment with parts contributes one clip per part. A `1.0`/`1.1` segment with no parts contributes exactly one clip carrying the segment's own bounds and artifact. `segmentIndex` is preserved on every clip so the player can still resolve the owning segment for the transcript, provenance, and the regenerate control.

Because the lesson clock spans the whole manifest while the audio element holds one clip at a time, lesson position is the clip's start offset plus the element's `currentTime`. Active segment and active sentence are both derived from that lesson position by scanning `parts`, so seeking, transcript auto-scroll, and the regenerate control all work at part granularity without the manifest carrying any word-level timing.

---

### Audio segment artifact

One `audio_segment` artifact per part. The envelope is the standard `Artifact` above; the payload is:

```json
{
  "schemaVersion": "1.1",
  "narrationArtifactId": "...",
  "segmentId": "s-1",
  "cacheKey": "<sha256>",
  "provider": "kokoro",
  "voice": "af_sky",
  "speed": 1.0,
  "mimeType": "audio/ogg",
  "durationMs": 1000,
  "byteLength": 23512,
  "ttsText": "The sentence this part speaks."
}
```

`cacheKey` is `sha256(provider \0 mimeType \0 voice \0 speed \0 segmentId \0 text)`. Because the MIME type is in the key, a switch from a WAV provider to an Opus provider never reuses a cached segment.

Cache resolution walks the job's artifacts in reverse insertion order and takes the **first** match, so the newest artifact for a key wins. This is what makes forced regeneration sticky: `regenerate_segment_audio` bypasses the cache for the named segment and persists a new artifact, and a later ordinary pass picks that newer artifact up rather than reverting to the original.

---

## Changing a contract

1. **Agree on the shape.** Prefer backwards-compatible changes (add optional fields) over breaking existing structures. Bumping a manifest `schemaVersion` obliges the reader to accept both versions, because manifests already on disk are immutable artifacts.
2. **Land in one change:** update this document, the Rust type in its owning module, and the TypeScript type in `src/App.tsx`.
3. **Update the unit test that pins the shape.** Manifest version, part timing, and cache-key derivation are all asserted directly in `src-tauri/src/audio.rs` and `src-tauri/tests/audio_generation.rs`.
