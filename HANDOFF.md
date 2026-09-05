# Digest implementation handoff

## Start here

- Treat [`article_learning_engine_prd.md`](article_learning_engine_prd.md) as the
  authoritative product and architecture document. The user explicitly considers
  the other existing repository documentation stale unless it is promoted into
  the PRD.
- The current branch is `main`. Review commits `15770d6` through `7c3ee11` rather
  than reconstructing their changes here. The latest relevant commits are:
  - `96f3910 fix: stream large run activity safely`
  - `8b9518b feat: generate and play segmented lesson audio`
  - `7c3ee11 fix: accept Kokoro streaming WAV lengths`
- V1 is a Linux Tauri quality-evaluation client using Chromium/WebView. OpenCode
  and agy are the initial ACP agents. V2 will replace the local actor/control
  topology with Hermes on a remote server without replacing compiler, artifact,
  manifest, or playback contracts.
- Use pnpm, not npm or yarn.
- Do not cap findings, narration segments, lesson duration, or model output.
  Rendering may use collapsed previews, but complete output must remain durable
  and inspectable.

## Current working state

The vertical slice currently supports:

1. URL capture, article extraction, normalized text blocks, image localization,
   and diagram detection.
2. ACP execution through OpenCode and an explicit agy adapter command.
3. Durable attempts, canonical activity, cancellation, inactivity supervision,
   interrupted-run recovery, and recent-run navigation.
4. MCP-authored analysis and narration plans with source-block accounting and
   TTS-faithfulness validation.
5. Cursor-based activity polling and expandable event previews, avoiding repeated
   transfer/rendering of multi-megabyte agent histories.
6. A provider-neutral audio boundary, Kokoro HTTP provider, per-segment audio
   caching, playback manifests, raw binary Tauri IPC, and synchronized React
   playback.

The primary implementation seams are:

- `src-tauri/src/application.rs`: durable artifact/event/attempt storage.
- `src-tauri/src/ingestion.rs`: source capture and normalization.
- `src-tauri/src/acp.rs`: ACP lifecycle and supervision.
- `src-tauri/src/tools.rs`: validated MCP contracts.
- `src-tauri/src/audio.rs`: audio provider, caching, WAV validation, and manifest
  generation.
- `src-tauri/src/host.rs`: Tauri command boundary.
- `src/App.tsx` and `src/App.css`: dark evaluation UI and player.

All checks passed after `7c3ee11`: 34 Rust tests, clippy with warnings denied,
TypeScript/Vite production build, and the Tauri release build.

## Latest real evaluation

The latest quality run is `evaluation-2ac91f14` in the local application database
at `$HOME/.local/share/com.bhondu.digest/digest.db`.

It completed with:

- 90 normalized source blocks and 7,078 source words;
- 60 analysis findings;
- 31 narration segments and 4,576 narration words;
- every source block explicitly taught, summarized, or skipped;
- all nine diagram blocks represented;
- 31 Kokoro audio artifacts and one playback manifest;
- 1,735.6 seconds of audio (28m55.6s), averaging 158.2 spoken words/minute.

The live audio files were technically healthy: mono 24 kHz float32 WAV, stable
levels, no clipping, and no non-finite samples. The first Kokoro response exposed
that its WAV endpoint writes `0xFFFFFFFF` for unknown RIFF/data lengths; `7c3ee11`
adds and tests the correct data-to-EOF behavior.

## Runtime and resource findings

Kokoro was run from `ghcr.io/lucasjinreal/kokoros:main` and reached through its
OpenAI-compatible endpoint at `http://127.0.0.1:3000`. Override this with
`DIGEST_KOKORO_URL`.

Observed for the 29-minute lesson:

- generation completed at roughly 3.3× realtime;
- Kokoro retained about 2.3 GiB RSS after generation;
- Digest plus WebKit processes used roughly 0.5 GiB RSS;
- raw WAV artifacts occupied 158.9 MiB;
- the entire Digest data directory occupied about 173 MiB;
- kernel logs showed no OOM kill, segmentation fault, or process trap.

The shell's `SIGHUPed` warning referred to a shell-managed background job and did
not corrupt the completed run. Durable playback does not need Kokoro to remain
running after generation.

## Recommended next slice

Implement the next Phase 1 audio/runtime increment already recorded in the PRD:

1. Add generation progress and cancellation at the segment boundary. Preserve
   already completed cached segments when cancellation or provider failure occurs.
2. Replace durable float32 WAV with a Chromium-compatible compressed format,
   preferably Ogg Opus, while deriving trustworthy durations before committing
   the manifest. A 64–96 kbps speech encoding should reduce the observed lesson
   from about 159 MiB to roughly 14–21 MiB.
3. Subdivide long narration segments at sentence boundaries for generation,
   seeking, alignment, and partial regeneration. This is transport subdivision,
   not a model-output or content limit.
4. Add sentence or word alignment. Kokoros documents a timestamped ONNX model and
   TSV sidecars for its CLI; confirm whether its HTTP API exposes equivalent
   timing before choosing HTTP extension versus a CLI provider.
5. Add player auto-scroll, keyboard controls, and per-segment regeneration.
6. Consider managed Kokoro lifecycle/idle shutdown only after generation
   cancellation and progress semantics are stable.

Primary Kokoros reference:
<https://github.com/lucasjinreal/Kokoros>.

## Repository hygiene

At handoff time, these pre-existing user changes remain outside the implementation
commits:

```text
 D .vscode/extensions.json
A  docs/.vscode/extensions.json
A  "docs/add wiki based pronunciation for short f"
```

Do not discard, rewrite, or accidentally include them. When committing, use
explicit paths or `git commit --only`.

## Suggested skills

The next agent should load:

- `implement` for the next PRD slice and required final commit;
- `tdd` for compressed-audio duration parsing, cancellation, caching, and
  playback state seams;
- `find-docs` for current Kokoros HTTP/timestamp behavior, Opus container details,
  and any Tauri raw-media API questions;
- `impeccable` for player progress, regeneration, responsive layout, keyboard
  interaction, and accessibility;
- `diagnosing-bugs` when validating against the live Kokoro container;
- `blast-radius` before changing artifact/manifest formats that existing runs and
  V2 Hermes must continue to consume;
- `code-review` after implementation and before committing.
