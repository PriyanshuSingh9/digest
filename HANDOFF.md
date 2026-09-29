# Digest implementation handoff

## Start here

- Treat [`article_learning_engine_prd.md`](article_learning_engine_prd.md) as the
  authoritative product and architecture document. The user explicitly considers
  the other existing repository documentation stale unless it is promoted into
  the PRD.
- The current branch is `main`. HEAD is `3d99c32`. Review commits `15770d6`
  through `3d99c32` rather than reconstructing their changes here. The latest
  relevant commits are:
  - `3d99c32 feat: implement Kokoro audio pipeline with sentence subdivision and player controls`
  - `fb5a6e9 feat: supervise active agent runs and stream activity safely`
  - `1f2322e fix: make lesson compression accountable`
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
6. A provider-neutral audio boundary, Kokoro HTTP provider delivering Ogg Opus,
   per-sentence-part audio caching, playback manifests, raw binary Tauri IPC, and
   synchronized React playback.
7. Segment-boundary generation progress, cooperative cancellation that preserves
   completed cached segments, and per-segment forced regeneration.
8. A player that streams one sentence part at a time, derives lesson position
   from the audio clock, and auto-scrolls the transcript.

The primary implementation seams are:

- `src-tauri/src/application.rs`: durable artifact/event/attempt storage.
- `src-tauri/src/ingestion.rs`: source capture and normalization.
- `src-tauri/src/acp.rs`: ACP lifecycle and supervision.
- `src-tauri/src/tools.rs`: validated MCP contracts.
- `src-tauri/src/mcp.rs`: the MCP server the agent launches.
- `src-tauri/src/audio.rs`: audio provider, per-part cache, Ogg Opus/WAV duration
  derivation, sentence subdivision, and playback manifest `1.2`.
- `src-tauri/src/host.rs`: Tauri command boundary.
- `src/App.tsx` and `src/App.css`: dark evaluation UI and player.

All checks passed at `3d99c32`: 54 Rust tests passing and 1 ignored (the live
Kokoro test), clippy with warnings denied, TypeScript/Vite production build, and
the Tauri release build.

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

This run is the reference for audio quality, but not for storage or format: it
predates the Ogg Opus and sentence-subdivision work. The run's 31 segments and
1,735.6 seconds of audio are still a valid quality measurement, while the
158.9 MiB WAV footprint and the 31-artifact count no longer describe what the
pipeline produces. Manifests and audio from this run remain playable -- schema
1.0 and 1.1 normalize into the same clip model the current player uses.

## Runtime and resource findings

Kokoro was run from `ghcr.io/lucasjinreal/kokoros:main` and reached through its
OpenAI-compatible endpoint at `http://127.0.0.1:3000`. Override this with
`DIGEST_KOKORO_URL`.

Observed for the 29-minute lesson:

- generation completed at roughly 3.3x realtime;
- Kokoro retained about 2.3 GiB RSS after generation;
- Digest plus WebKit processes used roughly 0.5 GiB RSS;
- raw WAV artifacts occupied 158.9 MiB;
- the entire Digest data directory occupied about 173 MiB;
- kernel logs showed no OOM kill, segmentation fault, or process trap.

Those WAV figures are pre-Opus. `3d99c32` moved the durable format to Ogg Opus,
which is roughly a tenth of that size, but the generation-time CPU and Kokoro's
retained RSS are unaffected by the container choice and should be re-measured
rather than assumed.

The shell's `SIGHUPed` warning referred to a shell-managed background job and did
not corrupt the completed run. Durable playback does not need Kokoro to remain
running after generation.

## Recommended next slice

The first five increments from the previous handoff are done. What remains open
in the audio/runtime path:

1. **Word-level alignment.** Blocked on the provider, not on the design.
   Kokoros documents a timestamped ONNX model and TSV sidecars, but only for its
   CLI; its HTTP endpoint returns audio and no timings. Sentence granularity is
   currently delivered by transport subdivision in manifest `1.2`. Word timing
   needs either an HTTP extension upstream or a CLI-based provider. Re-verify
   upstream before building anything: this is the single largest open item in
   Phase 1.
2. **Managed Kokoro lifecycle and idle shutdown.** Deliberately deferred until
   generation cancellation and progress semantics were stable; they now are.
   `3d99c32` adds `cancel_audio_generation` and an `ActiveJobGuard` that refuses a
   second concurrent generation for a job, so the host is ready to supervise the
   provider process itself.
3. **Re-measure the 29-minute lesson on the Opus path.** The resource findings
   in this document were taken on float32 WAV. Regenerate that lesson as Ogg
   Opus and record the new artifact footprint, total data directory size,
   generation wall-clock, and part count. The storage claim should be a measured
   number, not the projected one in [D-012](docs/decisions.md#d-012--durable-audio-per-sentence-ogg-opus-artifacts-over-a-concatenated-master-stream).
4. **Presentation renderers.** Not audio, but the other half of the Phase 1
   runtime gap: `presentation` reaches the manifest as opaque JSON and the player
   reads only `presentation.type`. Diagrams, code walkthroughs, and charts are
   unbuilt.

A separate ingestion track is open in Phase 2 and is tracked in
`docs/milestones.md`: Chromium rendered-DOM fallback, a checked-in benchmark
fixture corpus, responsive-candidate selection, DNS-rebinding defenses, and
format-specific media decoding limits.

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
- `tdd` for any new duration parsing, cancellation, caching, or playback state
  seams;
- `find-docs` for current Kokoros HTTP/timestamp behavior, Opus container details,
  and any Tauri raw-media API questions;
- `impeccable` for the presentation renderers, responsive layout, keyboard
  interaction, and accessibility;
- `diagnosing-bugs` when validating against the live Kokoro container;
- `blast-radius` before changing artifact/manifest formats that existing runs and
  V2 Hermes must continue to consume;
- `code-review` after implementation and before committing.
