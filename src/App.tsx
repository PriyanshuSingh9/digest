import {
  KeyboardEvent,
  PointerEvent as ReactPointerEvent,
  ReactNode,
  SyntheticEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import "./App.css";

type Provider = "open_code" | "agy";
type AgentModel = {
  id: string;
  name: string;
  description: string | null;
  current: boolean;
};
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
  agentWorkspaceDir: string;
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
  presentation: { type?: string; visual?: VisualSpec | null };
  /** Projected figure for `image` segments (manifest schema 1.4+). */
  image?: SegmentFigure | null;
};
type SegmentFigure = {
  imageId: string;
  artifactId: string;
  mimeType: string;
  alt: string | null;
  caption: string | null;
  width: number | null;
  height: number | null;
};
/**
 * Authored visual content (narration schema 1.4+, manifest schema 1.3+).
 * The renderer draws exactly this; nothing is inferred. Segments from older
 * plans carry no visual and fall back to the narration text.
 */
type VisualSpec =
  | {
      type: "diagram";
      nodes: { id: string; label: string }[];
      edges: { from: string; to: string; label?: string }[];
    }
  | { type: "points"; items: string[] };
type PlaybackClip = PlaybackPart & { segmentIndex: number };
type PlaybackManifest = {
  schemaVersion?: string;
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
type AnalysisClaim = { text?: string; sourceBlocks?: string[]; kind?: string };
type AnalysisFinding = {
  category?: string;
  text?: string;
  sourceBlocks?: string[];
  kind?: string;
};
type Stage = "compose" | "observe" | "read" | "listen";

const EMPTY_SNAPSHOT: RunSnapshot = { attempts: [], events: [], artifacts: [] };
const EVENT_PREVIEW_LENGTH = 1600;
const SEEK_STEP_MS = 5_000;
const PAGE_SEEK_MS = 15_000;
const PLAYBACK_RATES = [0.75, 1, 1.25, 1.5, 2];
const FINDING_CATEGORY_ORDER = [
  "major_concept",
  "key_claim",
  "definition",
  "supporting_concept",
  "example",
  "comparison",
  "causal_relationship",
  "quantitative_claim",
  "code_example",
  "important_entity",
  "prerequisite",
  "difficult_section",
  "point_of_confusion",
  "visualization_opportunity",
];
const PROVENANCE_LABELS: Record<string, string> = {
  source_derived: "From source",
  ai_explanation: "Agent explanation",
  ai_inference: "Agent inference",
  generated_educational: "Generated lesson copy",
};
const STAGES: { id: Stage; label: string; blurb: string }[] = [
  {
    id: "compose",
    label: "Compose",
    blurb: "Choose a provider and hand Digest an article URL.",
  },
  {
    id: "observe",
    label: "Observe",
    blurb: "Canonical ACP and MCP activity, appended by cursor.",
  },
  {
    id: "read",
    label: "Read",
    blurb: "What the agent claims, and which source blocks back it.",
  },
  {
    id: "listen",
    label: "Listen",
    blurb: "Synthesized narration with sentence-level transport.",
  },
];

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
const listOf = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
const statusTone = (status: string) =>
  status === "completed" || status === "captured"
    ? "accent"
    : status === "running"
      ? "info"
      : status === "failed"
        ? "danger"
        : "quiet";
const provenanceLabel = (kind: string | undefined) =>
  (kind && PROVENANCE_LABELS[kind]) || (kind ? eventLabel(kind) : "Unattributed");

function formatClock(milliseconds: number) {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function formatCount(value: number) {
  return value.toLocaleString();
}

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

function groupFindings(findings: AnalysisFinding[]) {
  const groups = new Map<string, AnalysisFinding[]>();
  for (const finding of findings) {
    const key =
      typeof finding.category === "string" && finding.category
        ? finding.category
        : "uncategorized";
    const bucket = groups.get(key);
    if (bucket) bucket.push(finding);
    else groups.set(key, [finding]);
  }
  return [...groups.entries()]
    .map(([category, items]) => ({ category, items }))
    .sort((left, right) => {
      const leftIndex = FINDING_CATEGORY_ORDER.indexOf(left.category);
      const rightIndex = FINDING_CATEGORY_ORDER.indexOf(right.category);
      const leftRank =
        leftIndex < 0 ? FINDING_CATEGORY_ORDER.length : leftIndex;
      const rightRank =
        rightIndex < 0 ? FINDING_CATEGORY_ORDER.length : rightIndex;
      return leftRank - rightRank || left.category.localeCompare(right.category);
    });
}

function useReducedMotion() {
  const [reduced, setReduced] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return reduced;
}

function StatusPill({ status, label }: { status: string; label: string }) {
  return (
    <span className={`pill ${statusTone(status)}`}>
      <span className="status-dot" aria-hidden="true" />
      {label}
    </span>
  );
}

function PanelHeading({
  id,
  title,
  meta,
  actions,
}: {
  id: string;
  title: string;
  meta?: string;
  actions?: ReactNode;
}) {
  return (
    <div className="panel-heading">
      <div>
        <h2 id={id}>{title}</h2>
        {meta && <span>{meta}</span>}
      </div>
      {actions}
    </div>
  );
}

function Disclosure({
  id,
  label,
  count,
  defaultOpen = false,
  children,
}: {
  id: string;
  label: string;
  count: number;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="disclosure">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="disclosure-marker" aria-hidden="true" />
        <span className="disclosure-label">{label}</span>
        <span className="disclosure-count">{count}</span>
      </button>
      <div className="disclosure-body" id={id} hidden={!open}>
        {children}
      </div>
    </div>
  );
}

function BlockChips({ blocks }: { blocks: string[] }) {
  if (blocks.length === 0) return <span className="chip chip-empty">No blocks</span>;
  return (
    <span className="chips">
      {blocks.map((block) => (
        <span className="chip" key={block}>
          {block}
        </span>
      ))}
    </span>
  );
}

function ActivityMessage({ event }: { event: AgentEvent }) {
  const [expanded, setExpanded] = useState(false);
  const [raw, setRaw] = useState(false);
  const message = conciseMessage(event);
  const isStructured = event.message.trimStart().startsWith("{");
  const needsPreview = message.length > EVENT_PREVIEW_LENGTH;
  const visibleMessage =
    needsPreview && !expanded
      ? `${message.slice(0, EVENT_PREVIEW_LENGTH)}…`
      : message;

  return (
    <>
      <p>{visibleMessage}</p>
      {(needsPreview || isStructured) && (
        <div className="event-actions">
          {needsPreview && (
            <button
              className="link-button"
              type="button"
              aria-expanded={expanded}
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded
                ? "Collapse"
                : `Show all ${formatCount(message.length)} characters`}
            </button>
          )}
          {isStructured && (
            <button
              className="link-button"
              type="button"
              aria-expanded={raw}
              onClick={() => setRaw((value) => !value)}
            >
              {raw ? "Hide raw event" : "Raw event"}
            </button>
          )}
        </div>
      )}
      {raw && <pre className="event-raw">{event.message}</pre>}
    </>
  );
}

const VISUAL_NODE_WIDTH = 150;
const VISUAL_NODE_HEIGHT = 56;
const VISUAL_COLUMN_GAP = 64;
const VISUAL_ROW_GAP = 28;
const VISUAL_PAD = 16;
const VISUAL_LABEL_CHARS = 20;
const VISUAL_LABEL_LINES = 3;

function wrapVisualLabel(label: string): string[] {
  const words = label.split(/\s+/).filter((word) => word !== "");
  const lines: string[] = [];
  let line = "";
  for (const word of words) {
    const next = line === "" ? word : `${line} ${word}`;
    if (next.length > VISUAL_LABEL_CHARS && line !== "") {
      lines.push(line);
      line = word;
    } else {
      line = next;
    }
    if (lines.length === VISUAL_LABEL_LINES - 1 && line.length > VISUAL_LABEL_CHARS) {
      lines.push(`${line.slice(0, VISUAL_LABEL_CHARS - 1)}…`);
      return lines;
    }
  }
  if (line !== "") lines.push(line);
  return lines.slice(0, VISUAL_LABEL_LINES);
}

function layoutDiagram(spec: Extract<VisualSpec, { type: "diagram" }>) {
  const ids = spec.nodes.map((node) => node.id);
  const depth = new Map<string, number>(ids.map((id) => [id, 0]));
  for (let pass = 0; pass < ids.length; pass++) {
    for (const edge of spec.edges) {
      if (!depth.has(edge.from) || !depth.has(edge.to)) continue;
      const next = (depth.get(edge.from) ?? 0) + 1;
      if (next > (depth.get(edge.to) ?? 0)) depth.set(edge.to, next);
    }
  }
  const columns = new Map<number, string[]>();
  for (const id of ids) {
    const level = depth.get(id) ?? 0;
    const column = columns.get(level) ?? [];
    column.push(id);
    columns.set(level, column);
  }
  const levels = [...columns.keys()].sort((a, b) => a - b);
  const positions = new Map<string, { x: number; y: number }>();
  const rows = Math.max(...[...columns.values()].map((column) => column.length));
  levels.forEach((level, columnIndex) => {
    const column = columns.get(level) ?? [];
    column.forEach((id, rowIndex) => {
      positions.set(id, {
        x:
          VISUAL_PAD +
          columnIndex * (VISUAL_NODE_WIDTH + VISUAL_COLUMN_GAP),
        y:
          VISUAL_PAD +
          rowIndex * (VISUAL_NODE_HEIGHT + VISUAL_ROW_GAP),
      });
    });
  });
  const width =
    VISUAL_PAD * 2 +
    levels.length * VISUAL_NODE_WIDTH +
    Math.max(0, levels.length - 1) * VISUAL_COLUMN_GAP;
  const height =
    VISUAL_PAD * 2 +
    rows * VISUAL_NODE_HEIGHT +
    Math.max(0, rows - 1) * VISUAL_ROW_GAP;
  return { positions, width, height };
}

function DiagramFigure({
  spec,
  segmentId,
}: {
  spec: Extract<VisualSpec, { type: "diagram" }>;
  segmentId: string;
}) {
  const nodes = Array.isArray(spec.nodes) ? spec.nodes : [];
  const edges = Array.isArray(spec.edges) ? spec.edges : [];
  if (nodes.length === 0) return null;
  const { positions, width, height } = layoutDiagram({ ...spec, nodes, edges });
  const summary = nodes
    .map((node) => {
      const outgoing = edges
        .filter((edge) => edge.from === node.id)
        .map((edge) => edge.to)
        .join(", ");
      return outgoing === "" ? node.label : `${node.label} to ${outgoing}`;
    })
    .join("; ");
  return (
    <figure
      className="visual-diagram"
      role="img"
      aria-label={`Diagram for ${segmentId}: ${summary}`}
    >
      <svg
        viewBox={`0 0 ${width} ${height}`}
        width={width}
        height={height}
        aria-hidden="true"
      >
        <defs>
          <marker
            id={`arrow-${segmentId}`}
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            orient="auto-start-reverse"
          >
            <path d="M 0 1 L 9 5 L 0 9 z" className="visual-edge-head" />
          </marker>
        </defs>
        {edges.map((edge, index) => {
          const from = positions.get(edge.from);
          const to = positions.get(edge.to);
          if (!from || !to) return null;
          const x1 = from.x + VISUAL_NODE_WIDTH;
          const y1 = from.y + VISUAL_NODE_HEIGHT / 2;
          const x2 = to.x;
          const y2 = to.y + VISUAL_NODE_HEIGHT / 2;
          const midX = (x1 + x2) / 2;
          return (
            <g key={`${edge.from}-${edge.to}-${index}`}>
              <path
                d={`M ${x1} ${y1} L ${midX} ${y1} L ${midX} ${y2} L ${x2} ${y2}`}
                className="visual-edge"
                markerEnd={`url(#arrow-${segmentId})`}
              />
              {edge.label !== undefined && edge.label !== "" && (
                <text x={midX} y={Math.min(y1, y2) - 4} className="visual-edge-label">
                  {edge.label.length > 24
                    ? `${edge.label.slice(0, 23)}…`
                    : edge.label}
                </text>
              )}
            </g>
          );
        })}
        {nodes.map((node) => {
          const position = positions.get(node.id);
          if (!position) return null;
          const lines = wrapVisualLabel(node.label);
          return (
            <g key={node.id}>
              <rect
                x={position.x}
                y={position.y}
                width={VISUAL_NODE_WIDTH}
                height={VISUAL_NODE_HEIGHT}
                rx={8}
                className="visual-node"
              />
              <text
                x={position.x + VISUAL_NODE_WIDTH / 2}
                y={
                  position.y +
                  VISUAL_NODE_HEIGHT / 2 -
                  ((lines.length - 1) * 7) +
                  4
                }
                className="visual-node-label"
              >
                {lines.map((line, lineIndex) => (
                  <tspan
                    key={lineIndex}
                    x={position.x + VISUAL_NODE_WIDTH / 2}
                    dy={lineIndex === 0 ? 0 : 14}
                  >
                    {line}
                  </tspan>
                ))}
              </text>
            </g>
          );
        })}
      </svg>
    </figure>
  );
}

const figureUrls = new Map<string, string>();

function FigurePanel({ figure }: { figure: SegmentFigure }) {
  const [objectUrl, setObjectUrl] = useState<string | null>(
    () => figureUrls.get(figure.artifactId) ?? null,
  );
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (figureUrls.has(figure.artifactId)) {
      setObjectUrl(figureUrls.get(figure.artifactId) ?? null);
      setFailed(false);
      return;
    }
    let disposed = false;
    setObjectUrl(null);
    setFailed(false);
    invoke<ArrayBuffer>("audio_asset", { artifactId: figure.artifactId })
      .then((bytes) => {
        if (disposed) return;
        const url = URL.createObjectURL(
          new Blob([bytes], { type: figure.mimeType }),
        );
        figureUrls.set(figure.artifactId, url);
        setObjectUrl(url);
      })
      .catch(() => {
        if (!disposed) setFailed(true);
      });
    return () => {
      disposed = true;
    };
  }, [figure.artifactId, figure.mimeType]);

  const caption = figure.caption ?? figure.alt ?? `Figure ${figure.imageId}`;
  if (failed || figure.artifactId === "") {
    return (
      <figure className="visual-figure visual-figure-missing" aria-label={caption}>
        <span>{caption}</span>
      </figure>
    );
  }
  if (objectUrl === null) return null;
  return (
    <figure className="visual-figure">
      <img
        src={objectUrl}
        alt={caption}
        style={
          figure.width !== null &&
          figure.height !== null &&
          figure.width > 0 &&
          figure.height > 0
            ? { aspectRatio: `${figure.width} / ${figure.height}` }
            : undefined
        }
      />
      <figcaption>{caption}</figcaption>
    </figure>
  );
}

function VisualPanel({ segment }: { segment: PlaybackSegment }) {
  if (
    segment.presentation.type === "image" &&
    segment.image !== undefined &&
    segment.image !== null
  ) {
    return <FigurePanel figure={segment.image} />;
  }
  const visual = segment.presentation.visual ?? null;
  if (visual !== null && visual.type === "diagram") {
    return <DiagramFigure spec={visual} segmentId={segment.id} />;
  }
  if (visual !== null && visual.type === "points") {
    const items = Array.isArray(visual.items) ? visual.items : [];
    if (items.length === 0) return null;
    return (
      <figure className="visual-points" aria-label={`Key points for ${segment.id}`}>
        <ul>
          {items.map((item, index) => (
            <li key={`${segment.id}-point-${index}`}>{item}</li>
          ))}
        </ul>
      </figure>
    );
  }
  return null;
}

function LessonPlayer({
  manifest,
  onRegenerateSegment,
  regenerationDisabled,
  busySegmentId,
}: {
  manifest: PlaybackManifest;
  onRegenerateSegment: (segmentId: string) => void;
  regenerationDisabled: boolean;
  busySegmentId: string | null;
}) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const activeTranscriptRef = useRef<HTMLLIElement>(null);
  const pendingSeek = useRef(0);
  const wantsPlayback = useRef(false);
  const reducedMotion = useReducedMotion();
  const [clipIndex, setClipIndex] = useState(0);
  const [currentMs, setCurrentMs] = useState(0);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [loadedClipId, setLoadedClipId] = useState<string | null>(null);
  const [loadingClip, setLoadingClip] = useState(false);
  const [buffering, setBuffering] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [fetchedMs, setFetchedMs] = useState(0);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [retryToken, setRetryToken] = useState(0);
  const [follow, setFollow] = useState(true);
  const [hoverMs, setHoverMs] = useState<number | null>(null);
  const [scrubbing, setScrubbing] = useState(false);

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
  const clip = clips[Math.min(clipIndex, Math.max(0, clips.length - 1))];
  const segmentIndex = clip?.segmentIndex ?? 0;
  const segment = manifest.segments[segmentIndex];
  const durationMs = Math.max(0, manifest.audio.durationMs);
  const positionMs = Math.min(currentMs, durationMs);
  const parts = segment?.parts?.length ? segment.parts : [];
  const activePartIndex = (() => {
    if (parts.length === 0) return -1;
    if (positionMs < parts[0].startMs) return 0;
    const found = parts.findIndex(
      (part) => part.endMs > positionMs && positionMs >= part.startMs,
    );
    return found < 0 ? parts.length - 1 : found;
  })();

  useEffect(() => {
    if (clips.length > 0 && clipIndex >= clips.length) {
      setClipIndex(clips.length - 1);
    }
  }, [clips.length, clipIndex]);

  useEffect(() => {
    const artifactId = clip?.audioArtifactId;
    if (!artifactId) {
      setAudioUrl(null);
      setLoadedClipId(null);
      setLoadingClip(false);
      return;
    }
    let disposed = false;
    let objectUrl: string | null = null;
    setAudioUrl(null);
    setLoadedClipId(null);
    setLoadError(null);
    setBuffering(false);
    setLoadingClip(true);
    setFetchedMs(0);
    invoke<ArrayBuffer>("audio_asset", { artifactId })
      .then((bytes) => {
        if (disposed) return;
        objectUrl = URL.createObjectURL(
          new Blob([bytes], { type: clip.mimeType ?? "audio/wav" }),
        );
        setAudioUrl(objectUrl);
        setLoadedClipId(artifactId);
      })
      .catch((reason) => {
        if (!disposed) setLoadError(String(reason));
      })
      .finally(() => {
        if (!disposed) setLoadingClip(false);
      });
    return () => {
      disposed = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [clip?.audioArtifactId, clip?.mimeType, retryToken]);

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

  useEffect(() => {
    if (!follow) return;
    activeTranscriptRef.current?.scrollIntoView({
      block: "nearest",
      behavior: reducedMotion ? "auto" : "smooth",
    });
  }, [segmentIndex, follow, reducedMotion]);

  function syncFetched() {
    const audio = audioRef.current;
    if (!audio || !clip || !audio.duration || loadedClipId !== clip.audioArtifactId) {
      return;
    }
    let end = 0;
    for (let index = 0; index < audio.buffered.length; index += 1) {
      if (audio.buffered.start(index) <= 0) {
        end = Math.max(end, audio.buffered.end(index));
      }
    }
    const span = Math.max(1, clip.endMs - clip.startMs);
    const loaded = clip.startMs + Math.min(1, end / audio.duration) * span;
    setFetchedMs(Math.min(clip.endMs, loaded));
  }

  function handleTimeUpdate(event: SyntheticEvent<HTMLAudioElement>) {
    if (!clip || loadedClipId !== clip.audioArtifactId) return;
    setCurrentMs(clip.startMs + event.currentTarget.currentTime * 1000);
    syncFetched();
  }

  function seek(targetMs: number) {
    if (clips.length === 0) return;
    const clamped = Math.max(0, Math.min(durationMs, targetMs));
    const foundIndex = clips.findIndex((item) => item.endMs > clamped);
    const normalizedIndex = foundIndex < 0 ? clips.length - 1 : foundIndex;
    const next = clips[normalizedIndex];
    pendingSeek.current = Math.max(0, clamped - next.startMs);
    setCurrentMs(clamped);
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
    if (target) seek(target.startMs);
  }

  function togglePlayback() {
    const audio = audioRef.current;
    if (!audio || !audioUrl) return;
    if (wantsPlayback.current) {
      wantsPlayback.current = false;
      audio.pause();
      setPlaying(false);
      setBuffering(false);
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

  function handleKeyDown(event: KeyboardEvent<HTMLElement>) {
    const target = event.target as HTMLElement;
    if (["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
    if (target.closest("[data-player-control]")) return;
    if (event.key === " " && target.tagName === "BUTTON") return;
    switch (event.key) {
      case " ":
        togglePlayback();
        break;
      case "ArrowLeft":
        seek(positionMs - SEEK_STEP_MS);
        break;
      case "ArrowRight":
        seek(positionMs + SEEK_STEP_MS);
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

  function handleScrubKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    let next = Number.NaN;
    switch (event.key) {
      case "ArrowLeft":
      case "ArrowDown":
        next = positionMs - SEEK_STEP_MS;
        break;
      case "ArrowRight":
      case "ArrowUp":
        next = positionMs + SEEK_STEP_MS;
        break;
      case "PageDown":
        next = positionMs - PAGE_SEEK_MS;
        break;
      case "PageUp":
        next = positionMs + PAGE_SEEK_MS;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = durationMs;
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
    seek(next);
  }

  function msFromPointer(event: ReactPointerEvent<HTMLDivElement>) {
    const track = event.currentTarget.getBoundingClientRect();
    if (track.width === 0) return 0;
    const ratio = Math.max(0, Math.min(1, (event.clientX - track.left) / track.width));
    return ratio * durationMs;
  }

  function handleScrubPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    event.currentTarget.setPointerCapture(event.pointerId);
    setScrubbing(true);
    seek(msFromPointer(event));
  }

  function handleScrubPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    const target = msFromPointer(event);
    setHoverMs(target);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) seek(target);
  }

  function endScrub(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    setScrubbing(false);
  }

  if (manifest.segments.length === 0) {
    return (
      <section className="lesson-player" aria-labelledby="lesson-player-title">
        <h2 id="lesson-player-title">{manifest.title}</h2>
        <p className="empty-note">
          The playback manifest for this run contains no segments. Generate
          lesson audio again to rebuild it.
        </p>
      </section>
    );
  }
  if (!segment || !clip) return null;

  const ratio = (ms: number) =>
    durationMs > 0
      ? `${Math.max(0, Math.min(100, (ms / durationMs) * 100))}%`
      : "0%";
  const playLabel = buffering
    ? "Buffering"
    : loadingClip
      ? "Loading"
      : playing
        ? "Pause"
        : "Play";
  const playBlocked = !audioUrl || loadingClip || Boolean(loadError);
  const playBlockedReason = loadError
    ? "This clip failed to load. Retry it before playing."
    : loadingClip
      ? "Loading the current clip."
      : "No audio is loaded yet.";

  return (
    <section
      className="lesson-player"
      aria-labelledby="lesson-player-title"
      tabIndex={0}
      onKeyDown={handleKeyDown}
      aria-keyshortcuts="Space ArrowLeft ArrowRight ArrowUp ArrowDown"
    >
      <div className="player-header">
        <div>
          <h2 id="lesson-player-title">{manifest.title}</h2>
          <p>
            {manifest.audio.provider} · voice {manifest.audio.voice} · schema{" "}
            {manifest.schemaVersion ?? "unversioned"}
          </p>
        </div>
        <div className="player-position">
          <span className="position-time">
            {formatClock(positionMs)}
            <span aria-hidden="true"> / </span>
            <span className="position-total">{formatClock(durationMs)}</span>
          </span>
          <span className="position-meta">
            Segment {segmentIndex + 1} of {manifest.segments.length}
            {parts.length > 0 && ` · sentence ${activePartIndex + 1} of ${parts.length}`}
          </span>
        </div>
      </div>

      <p className="visually-hidden" role="status">
        {`Segment ${segmentIndex + 1} of ${manifest.segments.length}`}
      </p>

      <div className="player-scrub">
        <div
          className="scrub-track"
          data-player-control
          role="slider"
          tabIndex={0}
          aria-label="Lesson position"
          aria-orientation="horizontal"
          aria-valuemin={0}
          aria-valuemax={Math.round(durationMs / 1000)}
          aria-valuenow={Math.round(positionMs / 1000)}
          aria-valuetext={`${formatClock(positionMs)} of ${formatClock(durationMs)}`}
          onKeyDown={handleScrubKeyDown}
          onPointerDown={handleScrubPointerDown}
          onPointerMove={handleScrubPointerMove}
          onPointerUp={endScrub}
          onPointerCancel={endScrub}
          onPointerLeave={() => setHoverMs(null)}
        >
          <span className="scrub-rail" aria-hidden="true" />
          <span
            className="scrub-fetched"
            style={{ width: ratio(fetchedMs) }}
            aria-hidden="true"
          />
          <span
            className="scrub-played"
            style={{ width: ratio(positionMs) }}
            aria-hidden="true"
          />
          <span
            className={`scrub-thumb${scrubbing ? " active" : ""}`}
            style={{ left: ratio(positionMs) }}
            aria-hidden="true"
          />
        </div>
        {hoverMs !== null && (
          <span
            className="scrub-preview"
            style={{ left: ratio(hoverMs) }}
            aria-hidden="true"
          >
            {formatClock(hoverMs)}
          </span>
        )}
      </div>

      <div className="player-chapters" aria-hidden="true">
        {manifest.segments.map((item, index) => (
          <span
            key={item.id}
            className={index === segmentIndex ? "chapter active" : "chapter"}
            style={{
              flexGrow: Math.max(0.001, item.endMs - item.startMs),
            }}
          />
        ))}
      </div>

      <div className="player-controls" data-player-control>
        <button
          className="icon-control"
          type="button"
          onClick={() => seek(positionMs - SEEK_STEP_MS)}
          disabled={positionMs <= 0}
          aria-label="Seek back 5 seconds"
          title="Seek back 5 seconds"
        >
          −5s
        </button>
        <button
          type="button"
          onClick={() => seekToSegment(segmentIndex - 1)}
          disabled={segmentIndex === 0}
        >
          Previous segment
        </button>
        <button
          className="primary-button"
          type="button"
          onClick={togglePlayback}
          disabled={playBlocked}
          title={playBlocked ? playBlockedReason : `${playing ? "Pause" : "Play"} the lesson (Space)`}
        >
          {playLabel}
        </button>
        <button
          type="button"
          onClick={() => seekToSegment(segmentIndex + 1)}
          disabled={segmentIndex === manifest.segments.length - 1}
        >
          Next segment
        </button>
        <button
          className="icon-control"
          type="button"
          onClick={() => seek(positionMs + SEEK_STEP_MS)}
          disabled={positionMs >= durationMs}
          aria-label="Seek forward 5 seconds"
          title="Seek forward 5 seconds"
        >
          +5s
        </button>
        <label className="rate-control">
          Speed
          <select
            value={playbackRate}
            onChange={(event) => setPlaybackRate(Number(event.currentTarget.value))}
          >
            {PLAYBACK_RATES.map((rate) => (
              <option value={rate} key={rate}>
                {rate}×
              </option>
            ))}
          </select>
        </label>
        <button
          className="follow-button"
          type="button"
          aria-pressed={follow}
          title={
            follow
              ? "The transcript follows the playing segment. Turn off to hold your place."
              : "The transcript holds your place. Turn on to follow the playing segment."
          }
          onClick={() => setFollow((value) => !value)}
        >
          Auto-scroll
        </button>
      </div>

      {loadError && (
        <div className="player-error" role="alert">
          <span>{loadError}</span>
          <button
            type="button"
            onClick={() => {
              setLoadError(null);
              setRetryToken((token) => token + 1);
            }}
          >
            Retry clip
          </button>
        </div>
      )}

      <article className="active-segment" aria-labelledby="active-segment-title">
        <div className="active-segment-meta">
          <h3 id="active-segment-title">
            {eventLabel(segment.presentation.type ?? "article_text")}
          </h3>
          <span className="segment-clock">
            {formatClock(segment.startMs)}–{formatClock(segment.endMs)}
          </span>
          <button
            type="button"
            className="segment-regenerate"
            onClick={() => onRegenerateSegment(segment.id)}
            disabled={regenerationDisabled}
            title="Regenerate this segment's audio, bypassing the cache"
          >
            {busySegmentId === segment.id ? "Regenerating…" : "Regenerate segment"}
          </button>
        </div>
        <VisualPanel segment={segment} />
        <p>{segment.displayText}</p>
        <div className="active-segment-blocks">
          <span>Source blocks</span>
          <BlockChips blocks={listOf(segment.sourceBlocks)} />
        </div>
      </article>

      <ol className="player-transcript" aria-label="Lesson transcript">
        {manifest.segments.map((item, index) => (
          <li
            key={item.id}
            ref={index === segmentIndex ? activeTranscriptRef : undefined}
          >
            <button
              type="button"
              aria-current={index === segmentIndex ? "true" : undefined}
              onClick={() => {
                setFollow(true);
                seek(item.startMs);
              }}
            >
              <span className="transcript-index">{index + 1}</span>
              <span className="transcript-clock">{formatClock(item.startMs)}</span>
              <span className="transcript-text">{item.displayText}</span>
            </button>
          </li>
        ))}
      </ol>

      <dl className="player-shortcuts">
        <div>
          <dt><kbd>Space</kbd></dt>
          <dd>Play or pause</dd>
        </div>
        <div>
          <dt><kbd>←</kbd><kbd>→</kbd></dt>
          <dd>Seek 5 seconds</dd>
        </div>
        <div>
          <dt><kbd>↑</kbd><kbd>↓</kbd></dt>
          <dd>Step segments</dd>
        </div>
      </dl>

      <audio
        ref={audioRef}
        src={audioUrl ?? undefined}
        preload="auto"
        onPlay={() => setPlaying(true)}
        onPlaying={() => setBuffering(false)}
        onPause={() => {
          setBuffering(false);
          if (!wantsPlayback.current) setPlaying(false);
        }}
        onWaiting={() => setBuffering(true)}
        onLoadedMetadata={syncFetched}
        onProgress={syncFetched}
        onTimeUpdate={handleTimeUpdate}
        onError={() =>
          setLoadError(
            audioRef.current?.error
              ? `Audio decode failed (${audioRef.current.error.code}).`
              : "Audio could not be decoded by this WebView.",
          )
        }
        onEnded={() => {
          if (clipIndex < clips.length - 1) {
            pendingSeek.current = 0;
            setClipIndex((index) => index + 1);
          } else {
            wantsPlayback.current = false;
            setPlaying(false);
            setCurrentMs(durationMs);
          }
        }}
      />
    </section>
  );
}

function App() {
  const [host, setHost] = useState<HostInfo | null>(null);
  const [hostError, setHostError] = useState<string | null>(null);
  const [jobId, setJobId] = useState(freshJobId);
  const [cwd, setCwd] = useState("");
  const [articleUrl, setArticleUrl] = useState("");
  const [provider, setProvider] = useState<Provider>("open_code");
  const [adapterCommand, setAdapterCommand] = useState("");
  const [model, setModel] = useState("");
  const [availableModels, setAvailableModels] = useState<AgentModel[] | null>(
    null,
  );
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [prompt, setPrompt] = useState(
    "Read the normalized article artifact. Identify its central argument and most important learning points with source-block provenance, then use Digest write_analysis exactly once.",
  );
  const [snapshot, setSnapshot] = useState<RunSnapshot>(EMPTY_SNAPSHOT);
  const [recentRuns, setRecentRuns] = useState<RunSummary[] | null>(null);
  const [selectedArtifact, setSelectedArtifact] = useState<string | null>(null);
  const [stage, setStage] = useState<Stage>("compose");
  const [stagePinned, setStagePinned] = useState(false);
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
  const [busySegmentId, setBusySegmentId] = useState<string | null>(null);
  const eventCursor = useRef<number | null>(null);
  const stageRefs = useRef<Partial<Record<Stage, HTMLButtonElement | null>>>({});
  const eventListRef = useRef<HTMLOListElement>(null);
  const stickToLatest = useRef(true);
  const wasRunning = useRef(false);
  const retryAction = useRef<(() => void) | null>(null);

  const clearError = () => {
    retryAction.current = null;
    setError(null);
  };

  const failWith = (reason: unknown, retry?: () => void) => {
    retryAction.current = retry ?? null;
    setError(String(reason));
  };

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
  const analysisView = useMemo(() => {
    const analysis = latestArtifact(snapshot.artifacts, "analysis");
    if (!analysis) return null;
    const claim = analysis.payload.centralArgument as AnalysisClaim | undefined;
    const rawFindings = analysis.payload.findings;
    const findings = Array.isArray(rawFindings)
      ? (rawFindings as AnalysisFinding[]).filter(
          (finding) => typeof finding?.text === "string",
        )
      : [];
    return {
      claim: typeof claim?.text === "string" ? claim : null,
      findings,
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

  const latestEvent = snapshot.events[snapshot.events.length - 1];
  const runStatus = running ? "running" : (runMetrics?.status ?? "idle");
  const runStatusLabel = running
    ? "Running"
    : runMetrics
      ? eventLabel(runMetrics.status)
      : "No attempt yet";

  const selectStage = useCallback((next: Stage) => {
    setStagePinned(true);
    setStage(next);
  }, []);

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
      failWith(reason);
    }
  }

  const refreshRecentRuns = useCallback(async () => {
    try {
      setRecentRuns(await invoke<RunSummary[]>("recent_runs", { limit: 20 }));
    } catch (reason) {
      setRecentRuns([]);
      setHostError(String(reason));
    }
  }, []);

  const refreshModels = useCallback(async () => {
    if (provider === "agy" && adapterCommand.trim() === "") {
      setAvailableModels(null);
      setModelsError("Enter the adapter executable before listing models.");
      return;
    }
    if (cwd.trim() === "") {
      setAvailableModels(null);
      setModelsError("Set a working directory before listing models.");
      return;
    }
    setModelsLoading(true);
    setModelsError(null);
    try {
      const models = await invoke<AgentModel[]>("list_agent_models", {
        input: {
          provider:
            provider === "open_code"
              ? { provider: "open_code" }
              : { provider: "agy", adapterCommand, adapterArgs: [] },
          cwd,
        },
      });
      setAvailableModels(models);
      setModel((current) =>
        current === "" || models.some((entry) => entry.id === current)
          ? current
          : "",
      );
    } catch (reason) {
      setAvailableModels(null);
      setModelsError(String(reason));
    } finally {
      setModelsLoading(false);
    }
  }, [provider, adapterCommand, cwd]);

  const loadHost = useCallback(async () => {
    setHostError(null);
    try {
      const info = await invoke<HostInfo>("host_info");
      setHost(info);
      setCwd((current) => (current === "" ? info.agentWorkspaceDir : current));
      await refreshRecentRuns();
    } catch (reason) {
      setHost(null);
      setHostError(
        "Digest must run inside its Tauri host. Start it with pnpm tauri dev.",
      );
    }
  }, [refreshRecentRuns]);

  const loadRun = useCallback(
    async (id: string) => {
      setJobId(id);
      clearError();
      setResult(null);
      setSelectedArtifact(null);
      setStagePinned(false);
      eventCursor.current = null;
      setSnapshot(EMPTY_SNAPSHOT);
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
        setStage("observe");
      } catch (reason) {
        failWith(
          new Error(`Could not load run ${id}: ${String(reason)}`),
          () => void loadRun(id),
        );
      }
    },
    [],
  );

  function createRun() {
    setJobId(freshJobId());
    setSnapshot(EMPTY_SNAPSHOT);
    eventCursor.current = null;
    setSelectedArtifact(null);
    setResult(null);
    clearError();
    setHostError(null);
    setRefreshSource(false);
    setStagePinned(true);
    setStage("compose");
  }

  useEffect(() => {
    void loadHost();
  }, [loadHost]);

  const lastModelProvider = useRef(provider);
  const hadModelCwd = useRef(false);
  useEffect(() => {
    const providerChanged = lastModelProvider.current !== provider;
    lastModelProvider.current = provider;
    const cwdArrived = !hadModelCwd.current && cwd.trim() !== "";
    hadModelCwd.current = cwd.trim() !== "";
    if (providerChanged || cwdArrived) void refreshModels();
  }, [provider, cwd, refreshModels]);

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

  useEffect(() => {
    if (wasRunning.current && !running && !stagePinned) {
      setStage(snapshot.attempts.length > 0 ? "read" : "observe");
    }
    wasRunning.current = running;
  }, [running, stagePinned, snapshot.attempts.length]);

  useEffect(() => {
    if (!running) return;
    const list = eventListRef.current;
    if (list && stickToLatest.current) list.scrollTop = list.scrollHeight;
  }, [running, snapshot.events.length]);

  function handleStageKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const index = STAGES.findIndex((item) => item.id === stage);
    let next = -1;
    if (event.key === "ArrowRight") next = (index + 1) % STAGES.length;
    else if (event.key === "ArrowLeft") {
      next = (index - 1 + STAGES.length) % STAGES.length;
    } else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = STAGES.length - 1;
    else return;
    event.preventDefault();
    const target = STAGES[next];
    selectStage(target.id);
    stageRefs.current[target.id]?.focus();
  }

  async function startRun() {
    const runJobId = jobId;
    clearError();
    setResult(null);
    setSnapshot(EMPTY_SNAPSHOT);
    eventCursor.current = null;
    setSelectedArtifact(null);
    setStagePinned(false);
    setStage("observe");
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
            model: model === "" ? null : model,
            allowOncePermissions,
            refreshSource,
          },
        }),
      );
    } catch (reason) {
      failWith(reason, () => void startRun());
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
    clearError();
    try {
      await invoke("cancel_agent_run", { jobId: activeJobId });
    } catch (reason) {
      setCancelling(false);
      failWith(reason, () => void cancelRun());
    }
  }

  async function runLessonAudio(
    command: "generate_audio" | "regenerate_segment_audio",
    input: Record<string, unknown>,
    segmentId: string | null,
  ) {
    setGeneratingAudio(true);
    setBusySegmentId(segmentId);
    clearError();
    const onProgress = new Channel<AudioProgress>();
    onProgress.onmessage = (progress) => setAudioProgress(progress);
    try {
      const generated = await invoke<{ manifest: Artifact }>(command, {
        input,
        onProgress,
      });
      await refreshSnapshot(jobId);
      setSelectedArtifact(generated.manifest.artifactId);
    } catch (reason) {
      failWith(reason, () => void runLessonAudio(command, input, segmentId));
      await refreshSnapshot(jobId).catch(() => {});
    } finally {
      setGeneratingAudio(false);
      setCancellingAudio(false);
      setAudioProgress(null);
      setBusySegmentId(null);
    }
  }

  async function generateLessonAudio() {
    await runLessonAudio("generate_audio", { jobId, speed: 1 }, null);
  }

  async function regenerateSegmentAudio(segmentId: string) {
    await runLessonAudio(
      "regenerate_segment_audio",
      { jobId, segmentId, speed: 1 },
      segmentId,
    );
  }

  async function cancelLessonAudio() {
    setCancellingAudio(true);
    try {
      await invoke("cancel_audio_generation", { jobId });
    } catch (reason) {
      setCancellingAudio(false);
      failWith(reason, () => void cancelLessonAudio());
    }
  }

  const stageBadges: Record<Stage, string | null> = {
    compose: null,
    observe: snapshot.events.length > 0 ? formatCount(snapshot.events.length) : null,
    read:
      runMetrics && runMetrics.findingCount > 0
        ? formatCount(runMetrics.findingCount)
        : null,
    listen:
      runMetrics && runMetrics.segmentCount > 0
        ? formatCount(runMetrics.segmentCount)
        : null,
  };
  const activeStageMeta = STAGES.find((item) => item.id === stage);
  const startBlockedReason = !host
    ? "The local host is not connected yet."
    : !allowOncePermissions
      ? "Allow one-time agent actions to start this run."
      : null;
  const coveragePercent = runMetrics?.sourceCoveragePercent ?? null;
  const findingGroups = analysisView ? groupFindings(analysisView.findings) : [];

  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">D</span>
          <div>
            <strong>Digest</strong>
            <span>Evaluation control plane</span>
          </div>
        </div>
        <div className="topbar-run">
          <span className="topbar-run-id">{jobId}</span>
          <StatusPill status={runStatus} label={runStatusLabel} />
        </div>
        <p className="host-state" role="status">
          <span
            className={`status-dot ${host ? "ready" : hostError ? "failed" : ""}`}
            aria-hidden="true"
          />
          {host ? "Local host ready" : hostError ? "Host unavailable" : "Connecting to host"}
        </p>
      </header>

      <div className="shell">
        {(hostError || error) && (
          <div className="notices">
            {hostError && (
              <div className="notice" role="alert">
                <strong>Host unavailable</strong>
                <span>{hostError}</span>
                <button type="button" onClick={() => void loadHost()}>
                  Retry connection
                </button>
              </div>
            )}
            {error && (
              <div className="notice" role="alert">
                <strong>Action failed</strong>
                <span>{error}</span>
                {retryAction.current && (
                  <button
                    type="button"
                    onClick={() => {
                      const retry = retryAction.current;
                      clearError();
                      retry?.();
                    }}
                  >
                    Retry
                  </button>
                )}
                <button type="button" onClick={clearError}>
                  Dismiss
                </button>
              </div>
            )}
          </div>
        )}

        <nav className="rail" aria-labelledby="rail-title">
          <div className="panel-heading">
            <div>
              <h2 id="rail-title">Runs</h2>
              {recentRuns && (
                <span>{formatCount(recentRuns.length)} recent</span>
              )}
            </div>
            <button
              className="secondary-button"
              type="button"
              onClick={createRun}
              disabled={running}
            >
              New run
            </button>
          </div>
          {recentRuns === null ? (
            <p className="empty-note">Loading saved runs…</p>
          ) : recentRuns.length === 0 ? (
            <p className="empty-note">
              No saved runs yet. Compose one from an article URL and it will be
              listed here.
            </p>
          ) : (
            <ul className="recent-run-list">
              {recentRuns.map((run) => (
                <li key={run.jobId}>
                  <button
                    type="button"
                    className={run.jobId === jobId ? "selected" : ""}
                    aria-current={run.jobId === jobId ? "true" : undefined}
                    onClick={() => void loadRun(run.jobId)}
                    disabled={running}
                  >
                    <span
                      className={`status-dot ${run.status}`}
                      aria-hidden="true"
                    />
                    <span className="recent-run-copy">
                      <strong>{run.title ?? run.jobId}</strong>
                      <small>
                        {eventLabel(run.status)} ·{" "}
                        {new Date(run.updatedAtMs).toLocaleString()}
                      </small>
                    </span>
                    <span className="recent-run-count">
                      {run.artifactCount}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <dl className="host-details">
            <div><dt>Artifact store</dt><dd title={host?.dataDir}>{host?.dataDir ?? "Unavailable"}</dd></div>
            <div><dt>Speech provider</dt><dd title={host?.kokoroEndpoint}>{host?.audioProvider ?? "Unavailable"}</dd></div>
            <div><dt>Default voice</dt><dd>{host?.audioDefaultVoice ?? "Unavailable"}</dd></div>
            <div>
              <dt>Inactivity timeout</dt>
              <dd>{host ? `${host.agentInactivityTimeoutSeconds}s` : "Unavailable"}</dd>
            </div>
            <div>
              <dt>Maximum runtime</dt>
              <dd>{host?.agentMaxRuntimeSeconds ? `${host.agentMaxRuntimeSeconds}s` : "Disabled"}</dd>
            </div>
          </dl>
        </nav>

        <main className="stage" aria-labelledby="stage-title">
          <div className="stage-header">
            <h1 id="stage-title">{activeStageMeta?.label}</h1>
            <p>{activeStageMeta?.blurb}</p>
          </div>
          <div
            className="stage-tabs"
            role="tablist"
            aria-label="Evaluation stages"
            onKeyDown={handleStageKeyDown}
          >
            {STAGES.map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                id={`stage-tab-${item.id}`}
                aria-selected={stage === item.id}
                aria-controls="stage-panel"
                tabIndex={stage === item.id ? 0 : -1}
                ref={(node) => {
                  stageRefs.current[item.id] = node;
                }}
                className={stage === item.id ? "selected" : ""}
                onClick={() => selectStage(item.id)}
              >
                {item.label}
                {stageBadges[item.id] && (
                  <span className="stage-badge">{stageBadges[item.id]}</span>
                )}
              </button>
            ))}
          </div>

          <div
            className="stage-body"
            role="tabpanel"
            id="stage-panel"
            aria-labelledby={`stage-tab-${stage}`}
            tabIndex={0}
          >
            {stage === "compose" && (
              <form
                className="stack"
                onSubmit={(event) => {
                  event.preventDefault();
                  void startRun();
                }}
              >
                <p className="stage-note">
                  Digest captures the article, then the agent authors the
                  analysis and narration plan through Digest MCP tools. Both
                  artifacts are validated before the run ends.
                </p>
                <div className="field">
                  <label htmlFor="job-id">Job ID</label>
                  <input
                    id="job-id"
                    value={jobId}
                    onChange={(event) => setJobId(event.currentTarget.value)}
                    required
                  />
                  <span className="field-help">
                    Every run is durable under this id. Reuse one to retry or
                    inspect it.
                  </span>
                </div>
                <div className="field">
                  <label htmlFor="provider">Agent provider</label>
                  <select
                    id="provider"
                    value={provider}
                    onChange={(event) =>
                      setProvider(event.currentTarget.value as Provider)
                    }
                  >
                    <option value="open_code">OpenCode</option>
                    <option value="agy">agy through ACP adapter</option>
                  </select>
                </div>
                {provider === "agy" && (
                  <div className="field">
                    <label htmlFor="adapter-command">Adapter executable</label>
                    <input
                      id="adapter-command"
                      value={adapterCommand}
                      onChange={(event) => setAdapterCommand(event.currentTarget.value)}
                      placeholder="/absolute/path/to/agy-acp"
                      required
                    />
                  </div>
                )}
                <div className="field">
                  <label htmlFor="agent-model">Agent model</label>
                  <div className="stage-action">
                    <select
                      id="agent-model"
                      value={model}
                      onChange={(event) => setModel(event.currentTarget.value)}
                      disabled={modelsLoading || availableModels === null}
                    >
                      <option value="">
                        Harness default
                        {availableModels?.find((entry) => entry.current)
                          ? ` (${availableModels.find((entry) => entry.current)?.name})`
                          : ""}
                      </option>
                      {(availableModels ?? []).map((entry) => (
                        <option key={entry.id} value={entry.id} title={entry.description ?? undefined}>
                          {entry.name}
                          {entry.current ? " (default)" : ""}
                        </option>
                      ))}
                    </select>
                    <button
                      className="secondary-button"
                      type="button"
                      onClick={() => void refreshModels()}
                      disabled={modelsLoading}
                    >
                      {modelsLoading ? "Loading…" : "Refresh"}
                    </button>
                  </div>
                  <span className="field-help">
                    {modelsError ??
                      "Models come from the harness itself, so this list can never go stale. Leave it on the harness default to skip selection."}
                  </span>
                </div>
                <div className="field">
                  <label htmlFor="article-url">Article URL</label>
                  <input
                    id="article-url"
                    type="url"
                    value={articleUrl}
                    onChange={(event) => setArticleUrl(event.currentTarget.value)}
                    placeholder="https://example.com/article"
                    required
                  />
                  <span className="field-help">
                    Digest captures the source before the agent starts.
                  </span>
                </div>
                <div className="field">
                  <label htmlFor="working-directory">Working directory</label>
                  <input
                    id="working-directory"
                    value={cwd}
                    onChange={(event) => setCwd(event.currentTarget.value)}
                    placeholder="/absolute/path/to/evaluation-workspace"
                    required
                  />
                  <span className="field-help">
                    ACP requires an absolute directory visible to the agent.
                  </span>
                </div>
                <div className="field">
                  <label htmlFor="evaluation-prompt">Evaluation prompt</label>
                  <textarea
                    id="evaluation-prompt"
                    value={prompt}
                    onChange={(event) => setPrompt(event.currentTarget.value)}
                    rows={6}
                    required
                  />
                </div>
                <div className="toggles">
                  <label className="check-control">
                    <input
                      type="checkbox"
                      checked={allowOncePermissions}
                      onChange={(event) =>
                        setAllowOncePermissions(event.currentTarget.checked)
                      }
                    />
                    <span>
                      Allow one-time agent actions for this run. Persistent
                      permissions are never granted.
                    </span>
                  </label>
                  <label className="check-control">
                    <input
                      type="checkbox"
                      checked={refreshSource}
                      onChange={(event) => setRefreshSource(event.currentTarget.checked)}
                    />
                    <span>
                      Fetch a fresh source capture. Leave off to reuse this
                      job&apos;s latest immutable article when retrying.
                    </span>
                  </label>
                </div>
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
                  <div className="stage-action">
                    <button
                      className="primary-button"
                      disabled={!host || !allowOncePermissions}
                    >
                      Start evaluation
                    </button>
                    {startBlockedReason && (
                      <span className="field-help">{startBlockedReason}</span>
                    )}
                  </div>
                )}
              </form>
            )}

            {stage === "observe" && (
              <div className="stack">
                <div className="panel-heading">
                  <div>
                    <h2>Activity</h2>
                    <span>
                      {formatCount(snapshot.events.length)} loaded
                      {running ? " · polling every 700ms" : ""}
                    </span>
                  </div>
                  <div className="panel-actions">
                    {running && !stickToLatest.current && (
                      <button
                        className="secondary-button"
                        type="button"
                        onClick={() => {
                          stickToLatest.current = true;
                          const list = eventListRef.current;
                          if (list) list.scrollTop = list.scrollHeight;
                        }}
                      >
                        Jump to latest
                      </button>
                    )}
                    <button
                      className="secondary-button"
                      type="button"
                      onClick={() => void refreshSnapshot()}
                      disabled={!jobId || running}
                    >
                      Refresh
                    </button>
                  </div>
                </div>

                <p className="live-line">
                  {running && (
                    <span className="pulse" aria-hidden="true" />
                  )}
                  <span role="status">
                    {running
                      ? "Agent session in progress."
                      : runMetrics
                        ? `Last attempt ${eventLabel(runMetrics.status).toLowerCase()}.`
                        : "No agent session has run under this job id."}
                  </span>
                  {latestEvent && (
                    <span className="live-latest">
                      {" "}Latest event {eventLabel(latestEvent.kind).toLowerCase()}{" "}
                      at {new Date(latestEvent.createdAtMs).toLocaleTimeString()}.
                    </span>
                  )}
                </p>

                {snapshot.attempts.length > 0 && (
                  <ul className="attempt-strip" aria-label="Run attempts">
                    {snapshot.attempts.map((attempt, index) => (
                      <li key={attempt.attemptId} className="attempt">
                        <span
                          className={`status-dot ${attempt.status}`}
                          aria-hidden="true"
                        />
                        <span>
                          Attempt {index + 1} · {eventLabel(attempt.status)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}

                {snapshot.events.length === 0 ? (
                  <div className="empty-panel">
                    <h3>No agent activity yet</h3>
                    <p>
                      Start an evaluation from Compose. ACP session updates and
                      Digest MCP tool activity appear here in sequence.
                    </p>
                    <button
                      className="secondary-button"
                      type="button"
                      onClick={() => selectStage("compose")}
                    >
                      Go to Compose
                    </button>
                  </div>
                ) : (
                  <ol
                    className="event-list"
                    ref={eventListRef}
                    onScroll={(event) => {
                      const list = event.currentTarget;
                      stickToLatest.current =
                        list.scrollHeight - list.scrollTop - list.clientHeight < 40;
                    }}
                  >
                    {snapshot.events.map((event) => (
                      <li key={event.sequence}>
                        <span
                          className={`event-indicator ${event.kind}`}
                          aria-hidden="true"
                        />
                        <div>
                          <div className="event-meta">
                            <strong>{eventLabel(event.kind)}</strong>
                            <time
                              dateTime={new Date(event.createdAtMs).toISOString()}
                            >
                              {new Date(event.createdAtMs).toLocaleTimeString()}
                            </time>
                          </div>
                          <ActivityMessage event={event} />
                        </div>
                      </li>
                    ))}
                  </ol>
                )}
              </div>
            )}

            {stage === "read" && (
              <div className="stack">
                {articleQuality && (
                  <section className="panel" aria-labelledby="capture-title">
                    <div className="panel-heading">
                      <div>
                        <h2 id="capture-title">Capture quality</h2>
                        <span>
                          {formatCount(articleQuality.diagnostics.blockCount)}{" "}
                          blocks ·{" "}
                          {formatCount(articleQuality.diagnostics.wordCount)}{" "}
                          words · {articleQuality.diagnostics.diagramCount ?? 0}{" "}
                          diagrams
                        </span>
                      </div>
                      <strong className="metric-figure">
                        {articleQuality.diagnostics.confidence}%
                      </strong>
                    </div>
                    <meter
                      min={0}
                      max={100}
                      low={45}
                      high={75}
                      optimum={100}
                      value={articleQuality.diagnostics.confidence}
                      aria-label={`Extraction confidence ${articleQuality.diagnostics.confidence} percent`}
                    >
                      {articleQuality.diagnostics.confidence}%
                    </meter>
                    <div className="inline-stats">
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
                      <ul className="warning-list">
                        {articleQuality.diagnostics.warnings.map((warning) => (
                          <li key={warning}>{warning}</li>
                        ))}
                      </ul>
                    )}
                  </section>
                )}

                {analysisView ? (
                  <>
                    {analysisView.claim && (
                      <section className="panel" aria-labelledby="claim-title">
                        <PanelHeading
                          id="claim-title"
                          title="Central argument"
                          meta={provenanceLabel(analysisView.claim.kind)}
                        />
                        <p className="claim-text">
                          {analysisView.claim.text}
                        </p>
                        <div className="block-row">
                          <span>Source blocks</span>
                          <BlockChips blocks={listOf(analysisView.claim.sourceBlocks)} />
                        </div>
                      </section>
                    )}
                    <section className="panel" aria-labelledby="findings-title">
                      <PanelHeading
                        id="findings-title"
                        title="Findings"
                        meta={`${formatCount(analysisView.findings.length)} across ${findingGroups.length} categories`}
                      />
                      {analysisView.findings.length === 0 ? (
                        <p className="empty-note">
                          The analysis artifact recorded no findings with text.
                        </p>
                      ) : (
                        <div className="disclosure-list">
                          {findingGroups.map((group, index) => (
                            <Disclosure
                              key={group.category}
                              id={`finding-group-${group.category}`}
                              label={eventLabel(group.category)}
                              count={group.items.length}
                              defaultOpen={index === 0}
                            >
                              <ul className="finding-list">
                                {group.items.map((finding, itemIndex) => (
                                  <li key={`${group.category}-${itemIndex}`}>
                                    <div className="finding-head">
                                      <span className="chip">
                                        {provenanceLabel(finding.kind)}
                                      </span>
                                      <BlockChips blocks={listOf(finding.sourceBlocks)} />
                                    </div>
                                    <p>{finding.text}</p>
                                  </li>
                                ))}
                              </ul>
                            </Disclosure>
                          ))}
                        </div>
                      )}
                    </section>
                  </>
                ) : (
                  <div className="empty-panel">
                    <h3>No analysis artifact yet</h3>
                    <p>
                      Analysis appears here once the agent has written and
                      validated it through the Digest MCP tool.
                    </p>
                  </div>
                )}

                <section className="panel" aria-labelledby="artifact-title">
                  <PanelHeading
                    id="artifact-title"
                    title="Artifacts"
                    meta={`${formatCount(inspectableArtifacts.length)} durable`}
                  />
                  {inspectableArtifacts.length === 0 ? (
                    <p className="empty-note">
                      Validated MCP outputs appear here with their content hash.
                      Audio segments are excluded; they play in the Listen
                      stage.
                    </p>
                  ) : (
                    <>
                      <ul className="artifact-list">
                        {inspectableArtifacts.map((artifact) => (
                          <li key={artifact.artifactId}>
                            <button
                              type="button"
                              aria-current={
                                activeArtifact?.artifactId === artifact.artifactId
                                  ? "true"
                                  : undefined
                              }
                              onClick={() => setSelectedArtifact(artifact.artifactId)}
                            >
                              <strong>{eventLabel(artifact.kind)}</strong>
                              <span>
                                {artifact.schemaVersion} ·{" "}
                                {artifact.artifactId.slice(0, 9)}
                              </span>
                            </button>
                          </li>
                        ))}
                      </ul>
                      {activeArtifact && (
                        <div className="artifact-detail">
                          <dl>
                            <div><dt>Schema</dt><dd>{activeArtifact.schemaVersion}</dd></div>
                            <div><dt>Content hash</dt><dd title={activeArtifact.contentHash}>{activeArtifact.contentHash.slice(0, 16)}</dd></div>
                            <div><dt>Created</dt><dd>{new Date(activeArtifact.createdAtMs).toLocaleString()}</dd></div>
                          </dl>
                          <pre>{JSON.stringify(activeArtifact.payload, null, 2)}</pre>
                        </div>
                      )}
                    </>
                  )}
                </section>
              </div>
            )}

            {stage === "listen" && (
              <div className="stack">
                {narrationArtifact ? (
                  <div className="panel audio-action">
                    <div>
                      <strong>
                        {generatingAudio
                          ? "Generating lesson audio"
                          : playbackArtifact
                            ? "Lesson audio ready"
                            : "Generate lesson audio"}
                      </strong>
                      <span>
                        {generatingAudio && audioProgress
                          ? audioProgress.currentSegmentId
                            ? `Segment ${audioProgress.completedSegments + 1} of ${audioProgress.totalSegments} · ${audioProgress.currentSegmentId}`
                            : `${audioProgress.completedSegments} of ${audioProgress.totalSegments} segments done`
                          : playbackArtifact
                            ? `${formatCount(playbackManifest?.segments.length ?? 0)} segments · ${formatClock(playbackManifest?.audio.durationMs ?? 0)} · manifest ${playbackManifest?.schemaVersion ?? "unversioned"}`
                            : `${host?.audioProvider ?? "The speech provider"} synthesizes and caches each narration segment. Cancelling keeps every completed segment.`}
                      </span>
                      {generatingAudio && audioProgress && (
                        <div className="generation-progress" role="status">
                          <progress
                            max={audioProgress.totalSegments}
                            value={audioProgress.completedSegments}
                            aria-label="Audio generation progress"
                          />
                          <span>
                            {formatCount(audioProgress.generatedSegmentCount)}{" "}
                            synthesized ·{" "}
                            {formatCount(audioProgress.reusedSegmentCount)} from
                            cache
                          </span>
                        </div>
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
                        disabled={running || !host}
                      >
                        {playbackArtifact ? "Regenerate all" : "Generate audio"}
                      </button>
                    )}
                  </div>
                ) : null}

                {playbackManifest ? (
                  <LessonPlayer
                    manifest={playbackManifest}
                    onRegenerateSegment={(segmentId) =>
                      void regenerateSegmentAudio(segmentId)
                    }
                    regenerationDisabled={generatingAudio || running}
                    busySegmentId={busySegmentId}
                  />
                ) : (
                  <div className="empty-panel">
                    <h3>No playable lesson yet</h3>
                    <p>
                      {narrationArtifact
                        ? "Generate lesson audio to synthesize every narration segment and cache a playback manifest for this run."
                        : "The lesson plays from a narration plan. This run has not written one yet."}
                    </p>
                    {narrationArtifact && !generatingAudio && (
                      <button
                        className="primary-button"
                        type="button"
                        onClick={() => void generateLessonAudio()}
                        disabled={running || !host}
                      >
                        Generate lesson audio
                      </button>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        </main>

        <aside className="inspector" aria-labelledby="inspector-title">
          <PanelHeading
            id="inspector-title"
            title="This run"
            meta={result ? `Session ended · ${eventLabel(result.stopReason)}` : undefined}
          />
          <dl className="run-identity">
            <div><dt>Job</dt><dd>{jobId}</dd></div>
            <div><dt>Provider</dt><dd>{provider === "open_code" ? "OpenCode" : "agy adapter"}</dd></div>
          </dl>
          {runMetrics ? (
            <dl className="metrics">
              <div>
                <dt>Findings</dt>
                <dd>{formatCount(runMetrics.findingCount)}</dd>
              </div>
              <div>
                <dt>Narration segments</dt>
                <dd>{formatCount(runMetrics.segmentCount)}</dd>
              </div>
              <div>
                <dt>Source blocks cited</dt>
                <dd>
                  {coveragePercent !== null ? `${coveragePercent}%` : "—"}
                </dd>
              </div>
              {coveragePercent !== null && (
                <div className="metric-bar-cell">
                  <span
                    className="metric-bar"
                    style={{ width: `${Math.max(0, Math.min(100, coveragePercent))}%` }}
                    aria-hidden="true"
                  />
                </div>
              )}
              {runMetrics.accountedBlockCount !== null && (
                <div>
                  <dt>Blocks accounted</dt>
                  <dd>
                    {formatCount(runMetrics.accountedBlockCount)}
                    <span className="metric-detail">
                      {runMetrics.taughtBlockCount} taught ·{" "}
                      {runMetrics.summarizedBlockCount} summarized ·{" "}
                      {runMetrics.skippedBlockCount} skipped
                    </span>
                  </dd>
                </div>
              )}
              {runMetrics.narrationWordCount !== null &&
                runMetrics.sourceWordCount !== null && (
                  <div>
                    <dt>Narration words</dt>
                    <dd>
                      {formatCount(runMetrics.narrationWordCount)} /{" "}
                      {formatCount(runMetrics.sourceWordCount)}
                      {runMetrics.narrationToSourceWordPercent !== null && (
                        <span className="metric-detail">
                          {runMetrics.narrationToSourceWordPercent}% of source
                        </span>
                      )}
                    </dd>
                  </div>
                )}
              <div>
                <dt>Diagrams referenced</dt>
                <dd>
                  {runMetrics.referencedDiagramCount} / {runMetrics.diagramBlockCount}
                </dd>
              </div>
            </dl>
          ) : (
            <p className="empty-note">
              No attempt recorded for this job id. Start one from Compose.
            </p>
          )}

          {snapshot.attempts.length > 0 && (
            <ul className="attempt-detail">
              {snapshot.attempts.map((attempt, index) => (
                <li key={attempt.attemptId}>
                  <div className="attempt-detail-head">
                    <StatusPill
                      status={attempt.status}
                      label={`Attempt ${index + 1} · ${eventLabel(attempt.status)}`}
                    />
                  </div>
                  <dl>
                    <div>
                      <dt>Provider</dt>
                      <dd>{eventLabel(attempt.provider)}</dd>
                    </div>
                    <div>
                      <dt>Started</dt>
                      <dd>
                        {new Date(attempt.startedAtMs).toLocaleTimeString()}
                      </dd>
                    </div>
                    {attempt.finishedAtMs !== null && (
                      <div>
                        <dt>Finished</dt>
                        <dd>
                          {new Date(attempt.finishedAtMs).toLocaleTimeString()}
                        </dd>
                      </div>
                    )}
                    {attempt.providerSessionId && (
                      <div>
                        <dt>Session</dt>
                        <dd title={attempt.providerSessionId}>
                          {attempt.providerSessionId.slice(0, 12)}
                        </dd>
                      </div>
                    )}
                  </dl>
                  {attempt.error && (
                    <p className="attempt-error">{attempt.error}</p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </aside>
      </div>
    </div>
  );
}

export default App;
