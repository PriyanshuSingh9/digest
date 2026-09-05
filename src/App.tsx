import {
  FormEvent,
  KeyboardEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import "./App.css";

type Provider = "open_code" | "agy";
type AgentEvent = {
  sequence: number;
  jobId: string;
  sessionId: string;
  kind: string;
  message: string;
  createdAtMs: number;
};
type Artifact = {
  schemaVersion: string;
  artifactId: string;
  jobId: string;
  kind: string;
  contentHash: string;
  createdAtMs: number;
  payload: Record<string, unknown>;
};
type RunAttempt = {
  attemptId: string;
  jobId: string;
  provider: Provider;
  providerSessionId: string | null;
  status: "running" | "completed" | "failed" | "cancelled";
  startedAtMs: number;
  finishedAtMs: number | null;
  error: string | null;
};
type RunSnapshot = {
  attempts: RunAttempt[];
  events: AgentEvent[];
  artifacts: Artifact[];
};
type HostInfo = {
  dataDir: string;
  mcpExecutable: string;
  agentInactivityTimeoutSeconds: number;
  agentMaxRuntimeSeconds: number | null;
  kokoroEndpoint: string;
  audioProvider: string;
  audioDefaultVoice: string;
};
type AgentRunResult = { sessionId: string; stopReason: string };
type AudioProgress = {
  totalSegments: number;
  completedSegments: number;
  generatedSegmentCount: number;
  reusedSegmentCount: number;
  currentSegmentId: string | null;
};
type PlaybackPart = {
  startMs: number;
  endMs: number;
  mimeType?: string;
  audioArtifactId: string;
  text?: string;
};
type PlaybackSegment = {
  id: string;
  startMs: number;
  endMs: number;
  /** Sentence-level transport parts (manifest schema 1.2+). */
  parts?: PlaybackPart[];
  /** Single-artifact segments from manifest schemas 1.0/1.1. */
  mimeType?: string;
  audioArtifactId?: string;
  displayText: string;
  sourceBlocks: string[];
  presentation: { type?: string };
};
type PlaybackClip = PlaybackPart & { segmentIndex: number };
type PlaybackManifest = {
  title: string;
  audio: {
    provider: string;
    voice: string;
    speed: number;
    durationMs: number;
  };
  segments: PlaybackSegment[];
};
type ExtractionDiagnostics = {
  confidence: number;
  wordCount: number;
  blockCount: number;
  imageCount: number;
  diagramCount?: number;
  warnings: string[];
};
type ArticleImage = {
  captureStatus: "pending" | "localized" | "failed";
};
type RunSummary = {
  jobId: string;
  title: string | null;
  provider: Provider | null;
  status: "captured" | "running" | "completed" | "failed" | "cancelled";
  updatedAtMs: number;
  artifactCount: number;
  eventCount: number;
};
type RunMetrics = {
  status: RunAttempt["status"];
  findingCount: number;
  segmentCount: number;
  sourceCoveragePercent: number | null;
  accountedBlockCount: number | null;
  taughtBlockCount: number | null;
  summarizedBlockCount: number | null;
  skippedBlockCount: number | null;
  narrationWordCount: number | null;
  sourceWordCount: number | null;
  narrationToSourceWordPercent: number | null;
  referencedDiagramCount: number;
  diagramBlockCount: number;
};

const EMPTY_SNAPSHOT: RunSnapshot = { attempts: [], events: [], artifacts: [] };
const EVENT_PREVIEW_LENGTH = 1600;
const freshJobId = () => `evaluation-${crypto.randomUUID().slice(0, 8)}`;
const eventLabel = (kind: string) =>
  kind
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
const latestArtifact = (artifacts: Artifact[], kind: string) =>
  [...artifacts].reverse().find((artifact) => artifact.kind === kind);
const numericField = (
  value: Record<string, unknown> | undefined,
  key: string,
) => (typeof value?.[key] === "number" ? value[key] as number : null);

function conciseMessage(event: AgentEvent) {
  if (!event.message.startsWith("{")) return event.message;
  try {
    const update = JSON.parse(event.message) as Record<string, unknown>;
    const content = update.content as Record<string, unknown> | undefined;
    if (typeof content?.text === "string") return content.text;
    if (typeof update.title === "string") return update.title;
  } catch {
    return event.message;
  }
  return eventLabel(event.kind);
}

function mergeEventUpdates(current: AgentEvent[], updates: AgentEvent[]) {
  const merged = [...current];
  for (const update of updates) {
    const previous = merged[merged.length - 1];
    const isStreamedText =
      update.kind === "agent_message" || update.kind === "agent_thinking";
    if (
      isStreamedText &&
      previous?.sessionId === update.sessionId &&
      previous.kind === update.kind
    ) {
      merged[merged.length - 1] = {
        ...update,
        message: previous.message + update.message,
      };
    } else {
      merged.push(update);
    }
  }
  return merged;
}

function ActivityMessage({ event }: { event: AgentEvent }) {
  const [expanded, setExpanded] = useState(false);
  const message = conciseMessage(event);
  const needsPreview = message.length > EVENT_PREVIEW_LENGTH;
  const visibleMessage =
    needsPreview && !expanded
      ? `${message.slice(0, EVENT_PREVIEW_LENGTH)}…`
      : message;

  return (
    <>
      <p>{visibleMessage}</p>
      {needsPreview && (
        <button
          className="event-expand"
          type="button"
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? "Collapse" : `Show all ${message.length.toLocaleString()} characters`}
        </button>
      )}
    </>
  );
}

function formatClock(milliseconds: number) {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function LessonPlayer({
  manifest,
  onRegenerateSegment,
  regenerationDisabled,
}: {
  manifest: PlaybackManifest;
  onRegenerateSegment: (segmentId: string) => void;
  regenerationDisabled: boolean;
}) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const activeTranscriptRef = useRef<HTMLLIElement>(null);
  const pendingSeek = useRef(0);
  const wantsPlayback = useRef(false);
  const [clipIndex, setClipIndex] = useState(0);
  const [currentMs, setCurrentMs] = useState(0);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [loadError, setLoadError] = useState<string | null>(null);
  // One playable clip per sentence part; legacy manifests (1.0/1.1) carry a
  // single artifact per segment and normalize to one clip.
  const clips = useMemo<PlaybackClip[]>(
    () =>
      manifest.segments.flatMap((segment, segmentIndex) =>
        (segment.parts && segment.parts.length > 0
          ? segment.parts
          : [
              {
                startMs: segment.startMs,
                endMs: segment.endMs,
                mimeType: segment.mimeType,
                audioArtifactId: segment.audioArtifactId ?? "",
              },
            ]
        ).map((part) => ({ ...part, segmentIndex })),
      ),
    [manifest],
  );
  const clip = clips[Math.min(clipIndex, clips.length - 1)];
  const segmentIndex = clip?.segmentIndex ?? 0;
  const segment = manifest.segments[segmentIndex];

  useEffect(() => {
    if (clips.length > 0 && clipIndex >= clips.length) {
      setClipIndex(clips.length - 1);
    }
  }, [clips.length, clipIndex]);

  useEffect(() => {
    if (!clip?.audioArtifactId) return;
    let disposed = false;
    let objectUrl: string | null = null;
    setAudioUrl(null);
    setLoadError(null);
    invoke<ArrayBuffer>("audio_asset", {
      artifactId: clip.audioArtifactId,
    })
      .then((bytes) => {
        if (disposed) return;
        objectUrl = URL.createObjectURL(
          new Blob([bytes], { type: clip.mimeType ?? "audio/wav" }),
        );
        setAudioUrl(objectUrl);
      })
      .catch((reason) => setLoadError(String(reason)));
    return () => {
      disposed = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [clip?.audioArtifactId]);

  useEffect(() => {
    activeTranscriptRef.current?.scrollIntoView({
      block: "nearest",
      behavior: "smooth",
    });
  }, [segmentIndex]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !audioUrl) return;
    audio.playbackRate = playbackRate;
    const resume = () => {
      audio.currentTime = pendingSeek.current / 1000;
      pendingSeek.current = 0;
      if (wantsPlayback.current) {
        void audio.play().catch((reason) => {
          wantsPlayback.current = false;
          setPlaying(false);
          setLoadError(String(reason));
        });
      }
    };
    audio.addEventListener("loadedmetadata", resume, { once: true });
    audio.load();
    return () => audio.removeEventListener("loadedmetadata", resume);
  }, [audioUrl]);

  useEffect(() => {
    if (audioRef.current) audioRef.current.playbackRate = playbackRate;
  }, [playbackRate]);

  function seek(targetMs: number) {
    const foundIndex = clips.findIndex((item) => item.endMs > targetMs);
    const normalizedIndex = foundIndex < 0 ? clips.length - 1 : foundIndex;
    const next = clips[normalizedIndex];
    pendingSeek.current = Math.max(0, targetMs - next.startMs);
    setCurrentMs(targetMs);
    if (normalizedIndex === clipIndex && audioRef.current) {
      audioRef.current.currentTime = pendingSeek.current / 1000;
      pendingSeek.current = 0;
    } else {
      setClipIndex(normalizedIndex);
    }
  }

  function seekToSegment(index: number) {
    const target = manifest.segments[
      Math.max(0, Math.min(manifest.segments.length - 1, index))
    ];
    seek(target.startMs);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLElement>) {
    const target = event.target as HTMLElement;
    if (["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
    if (event.key === " " && target.tagName === "BUTTON") return;
    switch (event.key) {
      case " ":
        togglePlayback();
        break;
      case "ArrowLeft":
        seek(Math.max(0, currentMs - 5_000));
        break;
      case "ArrowRight":
        seek(Math.min(manifest.audio.durationMs - 1, currentMs + 5_000));
        break;
      case "ArrowUp":
        seekToSegment(segmentIndex - 1);
        break;
      case "ArrowDown":
        seekToSegment(segmentIndex + 1);
        break;
      default:
        return;
    }
    event.preventDefault();
  }

  function togglePlayback() {
    const audio = audioRef.current;
    if (!audio) return;
    if (wantsPlayback.current) {
      wantsPlayback.current = false;
      audio.pause();
      setPlaying(false);
    } else {
      wantsPlayback.current = true;
      setPlaying(true);
      void audio.play().catch((reason) => {
        wantsPlayback.current = false;
        setPlaying(false);
        setLoadError(String(reason));
      });
    }
  }

  if (!segment || !clip) return null;
  return (
    <section
      className="lesson-player"
      aria-labelledby="lesson-player-title"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      aria-keyshortcuts="Space ArrowLeft ArrowRight ArrowUp ArrowDown"
    >
      <div className="player-heading">
        <div>
          <span className="context-label">Playable lesson</span>
          <h3 id="lesson-player-title">{manifest.title}</h3>
        </div>
        <span>{segmentIndex + 1} / {manifest.segments.length}</span>
      </div>
      <div className="player-timeline">
        <input
          aria-label="Lesson position"
          type="range"
          min={0}
          max={manifest.audio.durationMs}
          value={Math.min(currentMs, manifest.audio.durationMs)}
          onChange={(event) => seek(Number(event.currentTarget.value))}
        />
        <div>
          <span>{formatClock(currentMs)}</span>
          <span>{formatClock(manifest.audio.durationMs)}</span>
        </div>
      </div>
      <div className="player-controls">
        <button
          type="button"
          onClick={() => seekToSegment(segmentIndex - 1)}
          disabled={segmentIndex === 0}
        >
          Previous
        </button>
        <button
          className="primary-button"
          type="button"
          onClick={togglePlayback}
          disabled={!audioUrl}
        >
          {playing ? "Pause" : "Play"}
        </button>
        <button
          type="button"
          onClick={() => seekToSegment(segmentIndex + 1)}
          disabled={segmentIndex === manifest.segments.length - 1}
        >
          Next
        </button>
        <label>
          Speed
          <select
            value={playbackRate}
            onChange={(event) => setPlaybackRate(Number(event.currentTarget.value))}
          >
            {[0.75, 1, 1.25, 1.5, 2].map((rate) => (
              <option value={rate} key={rate}>{rate}×</option>
            ))}
          </select>
        </label>
      </div>
      <article className="active-segment" aria-live="polite">
        <div>
          <span>{eventLabel(segment.presentation.type ?? "article_text")}</span>
          <span>{segment.sourceBlocks.join(", ")}</span>
          <button
            type="button"
            className="segment-regenerate"
            onClick={() => onRegenerateSegment(segment.id)}
            disabled={regenerationDisabled}
            title="Regenerate this segment's audio, bypassing the cache"
          >
            Regenerate segment
          </button>
        </div>
        <p>{segment.displayText}</p>
      </article>
      <ol className="player-transcript" aria-label="Lesson transcript">
        {manifest.segments.map((item, index) => (
          <li
            key={item.id}
            ref={index === segmentIndex ? activeTranscriptRef : undefined}
          >
            <button
              type="button"
              aria-current={index === segmentIndex}
              onClick={() => seek(item.startMs)}
            >
              <span>{formatClock(item.startMs)}</span>
              <p>{item.displayText}</p>
            </button>
          </li>
        ))}
      </ol>
      {loadError && <p className="player-error">{loadError}</p>}
      <p className="player-hint">
        Space play/pause · ←/→ seek 5s · ↑/↓ segment
      </p>
      <audio
        ref={audioRef}
        src={audioUrl ?? undefined}
        onPlay={() => setPlaying(true)}
        onPause={() => {
          if (!wantsPlayback.current) setPlaying(false);
        }}
        onTimeUpdate={(event) =>
          setCurrentMs(clip.startMs + event.currentTarget.currentTime * 1000)
        }
        onEnded={() => {
          if (clipIndex < clips.length - 1) {
            pendingSeek.current = 0;
            setClipIndex((index) => index + 1);
          } else {
            wantsPlayback.current = false;
            setPlaying(false);
            setCurrentMs(manifest.audio.durationMs);
          }
        }}
      />
    </section>
  );
}

function App() {
  const [host, setHost] = useState<HostInfo | null>(null);
  const [jobId, setJobId] = useState(freshJobId);
  const [cwd, setCwd] = useState("");
  const [articleUrl, setArticleUrl] = useState("");
  const [provider, setProvider] = useState<Provider>("open_code");
  const [adapterCommand, setAdapterCommand] = useState("");
  const [prompt, setPrompt] = useState(
    "Read the normalized article artifact. Identify its central argument and most important learning points with source-block provenance, then use Digest write_analysis exactly once.",
  );
  const [snapshot, setSnapshot] = useState<RunSnapshot>(EMPTY_SNAPSHOT);
  const [recentRuns, setRecentRuns] = useState<RunSummary[]>([]);
  const [selectedArtifact, setSelectedArtifact] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [allowOncePermissions, setAllowOncePermissions] = useState(false);
  const [refreshSource, setRefreshSource] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AgentRunResult | null>(null);
  const [generatingAudio, setGeneratingAudio] = useState(false);
  const [audioProgress, setAudioProgress] = useState<AudioProgress | null>(null);
  const [cancellingAudio, setCancellingAudio] = useState(false);
  const eventCursor = useRef<number | null>(null);
  const narrationArtifact = latestArtifact(snapshot.artifacts, "narration_plan");
  const playbackArtifact = latestArtifact(snapshot.artifacts, "playback_manifest");
  const playbackManifest = playbackArtifact?.payload as PlaybackManifest | undefined;
  const inspectableArtifacts = useMemo(
    () => snapshot.artifacts.filter((artifact) => artifact.kind !== "audio_segment"),
    [snapshot.artifacts],
  );

  const activeArtifact = useMemo(
    () =>
      inspectableArtifacts.find(
        (artifact) => artifact.artifactId === selectedArtifact,
      ) ?? inspectableArtifacts[inspectableArtifacts.length - 1],
    [inspectableArtifacts, selectedArtifact],
  );
  const articleQuality = useMemo(() => {
    const article = latestArtifact(snapshot.artifacts, "normalized_article");
    if (!article || !article.payload.diagnostics) return null;
    return {
      diagnostics: article.payload.diagnostics as ExtractionDiagnostics,
      images: (article.payload.images ?? []) as ArticleImage[],
    };
  }, [snapshot.artifacts]);
  const runMetrics = useMemo<RunMetrics | null>(() => {
    const attempt = snapshot.attempts[snapshot.attempts.length - 1];
    if (!attempt) return null;
    const article = latestArtifact(snapshot.artifacts, "normalized_article");
    const analysis = latestArtifact(snapshot.artifacts, "analysis");
    const narration = latestArtifact(snapshot.artifacts, "narration_plan");
    const articleDiagnostics = article?.payload.diagnostics as
      | Record<string, unknown>
      | undefined;
    const narrationDiagnostics = narration?.payload.diagnostics as
      | Record<string, unknown>
      | undefined;
    return {
      status: attempt.status,
      findingCount: Array.isArray(analysis?.payload.findings)
        ? analysis.payload.findings.length
        : 0,
      segmentCount: Array.isArray(narration?.payload.segments)
        ? narration.payload.segments.length
        : 0,
      sourceCoveragePercent: numericField(
        narrationDiagnostics,
        "sourceCoveragePercent",
      ),
      accountedBlockCount: numericField(
        narrationDiagnostics,
        "accountedBlockCount",
      ),
      taughtBlockCount: numericField(narrationDiagnostics, "taughtBlockCount"),
      summarizedBlockCount: numericField(
        narrationDiagnostics,
        "summarizedBlockCount",
      ),
      skippedBlockCount: numericField(
        narrationDiagnostics,
        "skippedBlockCount",
      ),
      narrationWordCount: numericField(
        narrationDiagnostics,
        "narrationWordCount",
      ),
      sourceWordCount:
        numericField(narrationDiagnostics, "sourceWordCount") ??
        numericField(articleDiagnostics, "wordCount"),
      narrationToSourceWordPercent: numericField(
        narrationDiagnostics,
        "narrationToSourceWordPercent",
      ),
      referencedDiagramCount:
        numericField(narrationDiagnostics, "referencedDiagramCount") ?? 0,
      diagramBlockCount:
        numericField(narrationDiagnostics, "diagramBlockCount") ??
        numericField(articleDiagnostics, "diagramCount") ??
        0,
    };
  }, [snapshot]);

  async function refreshSnapshot(id = jobId, incremental = false) {
    if (!id.trim()) return;
    try {
      const afterEventSequence = incremental ? eventCursor.current : null;
      const loaded = await invoke<RunSnapshot>("run_snapshot", {
        jobId: id,
        afterEventSequence,
      });
      if (incremental && afterEventSequence !== null) {
        setSnapshot((current) => ({
          ...loaded,
          events: mergeEventUpdates(current.events, loaded.events),
        }));
      } else {
        setSnapshot(loaded);
      }
      eventCursor.current =
        loaded.events[loaded.events.length - 1]?.sequence ??
        afterEventSequence;
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function refreshRecentRuns() {
    try {
      setRecentRuns(await invoke<RunSummary[]>("recent_runs", { limit: 20 }));
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function loadRun(id: string) {
    setJobId(id);
    setError(null);
    setResult(null);
    setSelectedArtifact(null);
    eventCursor.current = null;
    try {
      const loaded = await invoke<RunSnapshot>("run_snapshot", { jobId: id });
      setSnapshot(loaded);
      eventCursor.current =
        loaded.events[loaded.events.length - 1]?.sequence ?? null;
      const article = [...loaded.artifacts]
        .reverse()
        .find((artifact) => artifact.kind === "normalized_article");
      if (typeof article?.payload.canonicalUrl === "string") {
        setArticleUrl(article.payload.canonicalUrl);
      }
      setRefreshSource(false);
    } catch (reason) {
      setError(String(reason));
    }
  }

  function createRun() {
    setJobId(freshJobId());
    setSnapshot(EMPTY_SNAPSHOT);
    eventCursor.current = null;
    setSelectedArtifact(null);
    setResult(null);
    setError(null);
    setRefreshSource(false);
  }

  useEffect(() => {
    invoke<HostInfo>("host_info")
      .then((info) => {
        setHost(info);
        void refreshRecentRuns();
      })
      .catch(() =>
        setError(
          "Digest must run inside its Tauri host. Start it with pnpm tauri dev.",
        ),
      );
  }, []);

  useEffect(() => {
    if (!running || !activeJobId) return;
    let stopped = false;
    let timer: number | undefined;
    const poll = async () => {
      await refreshSnapshot(activeJobId, true);
      if (!stopped) timer = window.setTimeout(poll, 700);
    };
    void poll();
    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [activeJobId, running]);

  async function startRun(event: FormEvent) {
    event.preventDefault();
    const runJobId = jobId;
    setError(null);
    setResult(null);
    setSnapshot(EMPTY_SNAPSHOT);
    eventCursor.current = null;
    setSelectedArtifact(null);
    setRunning(true);
    setActiveJobId(runJobId);
    const providerInput =
      provider === "open_code"
        ? { provider: "open_code" }
        : { provider: "agy", adapterCommand, adapterArgs: [] };

    try {
      setResult(
        await invoke<AgentRunResult>("start_agent_run", {
          input: {
            jobId: runJobId,
            cwd,
            articleUrl,
            prompt,
            provider: providerInput,
            allowOncePermissions,
            refreshSource,
          },
        }),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRunning(false);
      setActiveJobId(null);
      setCancelling(false);
      await refreshSnapshot(runJobId);
      await refreshRecentRuns();
    }
  }

  async function cancelRun() {
    if (!activeJobId) return;
    setCancelling(true);
    setError(null);
    try {
      await invoke("cancel_agent_run", { jobId: activeJobId });
    } catch (reason) {
      setCancelling(false);
      setError(String(reason));
    }
  }

  async function runLessonAudio(
    command: "generate_audio" | "regenerate_segment_audio",
    input: Record<string, unknown>,
  ) {
    setGeneratingAudio(true);
    setError(null);
    const onProgress = new Channel<AudioProgress>();
    onProgress.onmessage = (progress) => setAudioProgress(progress);
    try {
      const result = await invoke<{ manifest: Artifact }>(command, {
        input,
        onProgress,
      });
      await refreshSnapshot(jobId);
      setSelectedArtifact(result.manifest.artifactId);
    } catch (reason) {
      setError(String(reason));
      // Cancelled or failed generations keep completed segments cached.
      await refreshSnapshot(jobId).catch(() => {});
    } finally {
      setGeneratingAudio(false);
      setCancellingAudio(false);
      setAudioProgress(null);
    }
  }

  async function generateLessonAudio() {
    await runLessonAudio("generate_audio", { jobId, speed: 1 });
  }

  async function regenerateSegmentAudio(segmentId: string) {
    await runLessonAudio("regenerate_segment_audio", {
      jobId,
      segmentId,
      speed: 1,
    });
  }

  async function cancelLessonAudio() {
    setCancellingAudio(true);
    try {
      await invoke("cancel_audio_generation", { jobId });
    } catch (reason) {
      setCancellingAudio(false);
      setError(String(reason));
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">D</span>
          <div>
            <strong>Digest</strong>
            <span>Agent evaluation control plane</span>
          </div>
        </div>
        <div className="host-state">
          <span className={host ? "status-dot ready" : "status-dot"} />
          {host ? "Local host ready" : "Connecting to host"}
        </div>
      </header>

      <section className="workspace-heading">
        <div>
          <p className="context-label">Phase 1 · Quality vertical slice</p>
          <h1>Compile an article evaluation</h1>
          <p>
            Start an ACP session, observe its canonical activity, and inspect
            every durable artifact the agent creates through Digest MCP.
          </p>
        </div>
        {(runMetrics || result) && (
          <div className="completion-state" role="status">
            <span
              className={`status-dot ${
                runMetrics?.status === "completed" ? "ready" : ""
              }`}
            />
            {runMetrics ? (
              <span>
                {eventLabel(runMetrics.status)} · {runMetrics.findingCount} findings
                {" · "}{runMetrics.segmentCount} segments
                {runMetrics.sourceCoveragePercent !== null &&
                  ` · ${runMetrics.sourceCoveragePercent}% blocks cited`}
                {runMetrics.accountedBlockCount !== null &&
                  ` · ${runMetrics.taughtBlockCount} taught, ${runMetrics.summarizedBlockCount} summarized, ${runMetrics.skippedBlockCount} skipped`}
                {runMetrics.narrationWordCount !== null &&
                  runMetrics.sourceWordCount !== null &&
                  ` · ${runMetrics.narrationWordCount.toLocaleString()}/${runMetrics.sourceWordCount.toLocaleString()} words (${runMetrics.narrationToSourceWordPercent}%)`}
                {runMetrics.diagramBlockCount > 0 &&
                  ` · ${runMetrics.referencedDiagramCount}/${runMetrics.diagramBlockCount} diagrams`}
              </span>
            ) : (
              <>Session ended · {eventLabel(result!.stopReason)}</>
            )}
          </div>
        )}
      </section>

      {error && (
        <div className="error-banner" role="alert">
          <strong>Run needs attention</strong>
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <div className="workspace">
        <form className="run-form" onSubmit={startRun}>
          <section className="recent-runs" aria-labelledby="recent-runs-title">
            <div className="recent-runs-heading">
              <h2 id="recent-runs-title">Recent runs</h2>
              <button
                className="secondary-button"
                type="button"
                onClick={createRun}
                disabled={running}
              >
                New run
              </button>
            </div>
            {recentRuns.length === 0 ? (
              <p>No saved runs yet.</p>
            ) : (
              <div className="recent-run-list">
                {recentRuns.map((run) => (
                  <button
                    type="button"
                    key={run.jobId}
                    className={run.jobId === jobId ? "selected" : ""}
                    aria-current={run.jobId === jobId ? "page" : undefined}
                    onClick={() => void loadRun(run.jobId)}
                    disabled={running}
                  >
                    <span className={`status-dot ${run.status}`} />
                    <span className="recent-run-copy">
                      <strong>{run.title ?? run.jobId}</strong>
                      <small>
                        {run.provider ? eventLabel(run.provider) : "Captured"} ·{" "}
                        {new Date(run.updatedAtMs).toLocaleString()}
                      </small>
                    </span>
                    <span className="recent-run-count">
                      {run.artifactCount}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </section>
          <div className="section-title">
            <h2>Session setup</h2>
            <span>{provider === "open_code" ? "OpenCode ACP" : "agy adapter"}</span>
          </div>
          <label>
            Job ID
            <input value={jobId} onChange={(event) => setJobId(event.currentTarget.value)} required />
          </label>
          <label>
            Agent provider
            <select value={provider} onChange={(event) => setProvider(event.currentTarget.value as Provider)}>
              <option value="open_code">OpenCode</option>
              <option value="agy">agy through ACP adapter</option>
            </select>
          </label>
          {provider === "agy" && (
            <label>
              Adapter executable
              <input
                value={adapterCommand}
                onChange={(event) => setAdapterCommand(event.currentTarget.value)}
                placeholder="/absolute/path/to/agy-acp"
                required
              />
            </label>
          )}
          <label>
            Article URL
            <input
              type="url"
              value={articleUrl}
              onChange={(event) => setArticleUrl(event.currentTarget.value)}
              placeholder="https://example.com/article"
              required
            />
            <span className="field-help">
              Digest captures the source before the agent starts.
            </span>
          </label>
          <label className="permission-control">
            <input
              type="checkbox"
              checked={refreshSource}
              onChange={(event) => setRefreshSource(event.currentTarget.checked)}
            />
            <span>
              Fetch a fresh source capture. Leave off to reuse this job&apos;s
              latest immutable article when retrying.
            </span>
          </label>
          <label>
            Working directory
            <input
              value={cwd}
              onChange={(event) => setCwd(event.currentTarget.value)}
              placeholder="/absolute/path/to/evaluation-workspace"
              required
            />
            <span className="field-help">ACP requires an absolute directory visible to the agent.</span>
          </label>
          <label>
            Evaluation prompt
            <textarea value={prompt} onChange={(event) => setPrompt(event.currentTarget.value)} rows={7} required />
          </label>
          <label className="permission-control">
            <input
              type="checkbox"
              checked={allowOncePermissions}
              onChange={(event) => setAllowOncePermissions(event.currentTarget.checked)}
            />
            <span>
              Allow one-time agent actions for this run. Persistent permissions
              are never granted.
            </span>
          </label>
          {running ? (
            <button
              className="danger-button"
              type="button"
              onClick={() => void cancelRun()}
              disabled={cancelling}
            >
              {cancelling ? "Cancelling run…" : "Cancel run"}
            </button>
          ) : (
            <button className="primary-button" disabled={!host || !allowOncePermissions}>
              Start evaluation
            </button>
          )}
          <dl className="host-details">
            <div><dt>Artifact store</dt><dd title={host?.dataDir}>{host?.dataDir ?? "Unavailable"}</dd></div>
            <div><dt>MCP transport</dt><dd>stdio · bundled host</dd></div>
            <div>
              <dt>Inactivity timeout</dt>
              <dd>{host ? `${host.agentInactivityTimeoutSeconds}s` : "Unavailable"}</dd>
            </div>
            <div>
              <dt>Maximum runtime</dt>
              <dd>{host?.agentMaxRuntimeSeconds ? `${host.agentMaxRuntimeSeconds}s` : "Disabled"}</dd>
            </div>
            <div><dt>Kokoro</dt><dd title={host?.kokoroEndpoint}>{host?.kokoroEndpoint ?? "Unavailable"}</dd></div>
          </dl>
        </form>

        <section className="activity-panel" aria-labelledby="activity-title">
          <div className="section-title">
            <div>
              <h2 id="activity-title">Agent activity</h2>
              <span aria-live="polite">{running ? "Live" : `${snapshot.events.length} event${snapshot.events.length === 1 ? "" : "s"}`}</span>
            </div>
            <button className="secondary-button" type="button" onClick={() => void refreshSnapshot()} disabled={!jobId}>Refresh</button>
          </div>
          {snapshot.attempts.length > 0 && (
            <div className="attempt-strip" aria-label="Run attempts">
              {snapshot.attempts.map((attempt, index) => (
                <div key={attempt.attemptId} className={`attempt ${attempt.status}`}>
                  <span className="status-dot" />
                  <strong>Attempt {index + 1}</strong>
                  <span>{eventLabel(attempt.status)}</span>
                </div>
              ))}
            </div>
          )}
          {snapshot.events.length === 0 ? (
            <div className="empty-state">
              <span className="empty-glyph" aria-hidden="true">↳</span>
              <h3>No agent activity yet</h3>
              <p>Configure a provider and start an evaluation. ACP updates and MCP tool activity will appear here in sequence.</p>
            </div>
          ) : (
            <ol className="event-list">
              {snapshot.events.map((event) => (
                <li key={event.sequence}>
                  <span className={`event-indicator ${event.kind}`} />
                  <div>
                    <div className="event-meta">
                      <strong>{eventLabel(event.kind)}</strong>
                      <time dateTime={new Date(event.createdAtMs).toISOString()}>{new Date(event.createdAtMs).toLocaleTimeString()}</time>
                    </div>
                    <ActivityMessage event={event} />
                  </div>
                </li>
              ))}
            </ol>
          )}
        </section>

        <section className="artifact-panel" aria-labelledby="artifact-title">
          <div className="section-title">
            <h2 id="artifact-title">Artifacts</h2>
            <span>{snapshot.artifacts.length} durable</span>
          </div>
          {narrationArtifact && (
            <div className="audio-action">
              <div>
                <strong>{playbackArtifact ? "Lesson audio ready" : "Generate lesson audio"}</strong>
                <span>
                  {generatingAudio && audioProgress
                    ? audioProgress.currentSegmentId
                      ? `Segment ${audioProgress.completedSegments + 1} of ${audioProgress.totalSegments} · ${audioProgress.currentSegmentId}`
                      : `${audioProgress.completedSegments} of ${audioProgress.totalSegments} segments done`
                    : playbackArtifact
                      ? `${(playbackManifest?.segments.length ?? 0)} cached segments`
                      : `${host?.audioProvider ?? "The audio provider"} generates and caches each narration segment.`}
                </span>
                {generatingAudio && audioProgress && (
                  <progress
                    max={audioProgress.totalSegments}
                    value={audioProgress.completedSegments}
                    aria-label="Audio generation progress"
                  />
                )}
              </div>
              {generatingAudio ? (
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void cancelLessonAudio()}
                  disabled={cancellingAudio}
                >
                  {cancellingAudio ? "Cancelling…" : "Cancel"}
                </button>
              ) : (
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => void generateLessonAudio()}
                  disabled={running}
                >
                  {playbackArtifact ? "Regenerate" : "Generate audio"}
                </button>
              )}
            </div>
          )}
          {playbackManifest && (
            <LessonPlayer
              manifest={playbackManifest}
              onRegenerateSegment={(segmentId) =>
                void regenerateSegmentAudio(segmentId)
              }
              regenerationDisabled={generatingAudio || running}
            />
          )}
          {snapshot.artifacts.length === 0 ? (
            <div className="artifact-empty">Validated MCP outputs will appear here with their content hash.</div>
          ) : (
            <>
              {articleQuality && (
                <section className="quality-summary" aria-labelledby="quality-title">
                  <div className="quality-heading">
                    <div>
                      <h3 id="quality-title">Capture quality</h3>
                      <span>
                        {articleQuality.diagnostics.blockCount} blocks ·{" "}
                        {articleQuality.diagnostics.wordCount} words ·{" "}
                        {articleQuality.diagnostics.diagramCount ?? 0} diagrams
                      </span>
                    </div>
                    <strong>{articleQuality.diagnostics.confidence}%</strong>
                  </div>
                  <meter
                    min={0}
                    max={100}
                    low={45}
                    high={75}
                    optimum={100}
                    value={articleQuality.diagnostics.confidence}
                  >
                    {articleQuality.diagnostics.confidence}%
                  </meter>
                  <div className="quality-media">
                    <span>Media localized</span>
                    <strong>
                      {
                        articleQuality.images.filter(
                          (image) => image.captureStatus === "localized",
                        ).length
                      }
                      /{articleQuality.images.length}
                    </strong>
                  </div>
                  {articleQuality.diagnostics.warnings.length > 0 && (
                    <ul className="quality-warnings">
                      {articleQuality.diagnostics.warnings.map((warning) => (
                        <li key={warning}>{warning}</li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
              <div className="artifact-tabs" role="list">
                {inspectableArtifacts.map((artifact) => (
                  <button
                    type="button"
                    key={artifact.artifactId}
                    className={activeArtifact?.artifactId === artifact.artifactId ? "selected" : ""}
                    onClick={() => setSelectedArtifact(artifact.artifactId)}
                  >
                    <strong>{eventLabel(artifact.kind)}</strong>
                    <span>{artifact.artifactId.slice(0, 9)}</span>
                  </button>
                ))}
              </div>
              {activeArtifact && (
                <div className="artifact-detail">
                  <dl>
                    <div><dt>Schema</dt><dd>{activeArtifact.schemaVersion}</dd></div>
                    <div><dt>Content hash</dt><dd title={activeArtifact.contentHash}>{activeArtifact.contentHash.slice(0, 16)}</dd></div>
                  </dl>
                  <pre>{JSON.stringify(activeArtifact.payload, null, 2)}</pre>
                </div>
              )}
            </>
          )}
        </section>
      </div>
    </main>
  );
}

export default App;
