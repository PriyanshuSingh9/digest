import { FormEvent, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
type HostInfo = { dataDir: string; mcpExecutable: string };
type AgentRunResult = { sessionId: string; stopReason: string };
type ExtractionDiagnostics = {
  confidence: number;
  wordCount: number;
  blockCount: number;
  imageCount: number;
  warnings: string[];
};
type ArticleImage = {
  captureStatus: "pending" | "localized" | "failed";
};

const EMPTY_SNAPSHOT: RunSnapshot = { attempts: [], events: [], artifacts: [] };
const freshJobId = () => `evaluation-${crypto.randomUUID().slice(0, 8)}`;
const eventLabel = (kind: string) =>
  kind
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");

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

function App() {
  const [host, setHost] = useState<HostInfo | null>(null);
  const [jobId, setJobId] = useState(freshJobId);
  const [cwd, setCwd] = useState("");
  const [articleUrl, setArticleUrl] = useState("");
  const [provider, setProvider] = useState<Provider>("open_code");
  const [adapterCommand, setAdapterCommand] = useState("");
  const [prompt, setPrompt] = useState(
    "Read the normalized article artifact. Identify its central argument and most important learning points, then use Digest write_analysis exactly once.",
  );
  const [snapshot, setSnapshot] = useState<RunSnapshot>(EMPTY_SNAPSHOT);
  const [selectedArtifact, setSelectedArtifact] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [allowOncePermissions, setAllowOncePermissions] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AgentRunResult | null>(null);

  const activeArtifact = useMemo(
    () =>
      snapshot.artifacts.find(
        (artifact) => artifact.artifactId === selectedArtifact,
      ) ?? snapshot.artifacts[snapshot.artifacts.length - 1],
    [selectedArtifact, snapshot.artifacts],
  );
  const articleQuality = useMemo(() => {
    const article = snapshot.artifacts.find(
      (artifact) => artifact.kind === "normalized_article",
    );
    if (!article || !article.payload.diagnostics) return null;
    return {
      diagnostics: article.payload.diagnostics as ExtractionDiagnostics,
      images: (article.payload.images ?? []) as ArticleImage[],
    };
  }, [snapshot.artifacts]);

  async function refreshSnapshot(id = jobId) {
    if (!id.trim()) return;
    try {
      setSnapshot(await invoke<RunSnapshot>("run_snapshot", { jobId: id }));
    } catch (reason) {
      setError(String(reason));
    }
  }

  useEffect(() => {
    invoke<HostInfo>("host_info")
      .then(setHost)
      .catch(() =>
        setError(
          "Digest must run inside its Tauri host. Start it with pnpm tauri dev.",
        ),
      );
  }, []);

  useEffect(() => {
    if (!running) return;
    const poll = window.setInterval(() => void refreshSnapshot(), 700);
    return () => window.clearInterval(poll);
  }, [jobId, running]);

  async function startRun(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setResult(null);
    setSnapshot(EMPTY_SNAPSHOT);
    setSelectedArtifact(null);
    setRunning(true);
    const providerInput =
      provider === "open_code"
        ? { provider: "open_code" }
        : { provider: "agy", adapterCommand, adapterArgs: [] };

    try {
      setResult(
        await invoke<AgentRunResult>("start_agent_run", {
          input: {
            jobId,
            cwd,
            articleUrl,
            prompt,
            provider: providerInput,
            allowOncePermissions,
          },
        }),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRunning(false);
      await refreshSnapshot();
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
        {result && (
          <div className="completion-state" role="status">
            <span className="status-dot ready" />
            Session ended · {eventLabel(result.stopReason)}
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
          <div className="section-title">
            <h2>Session setup</h2>
            <span>{provider === "open_code" ? "OpenCode ACP" : "agy adapter"}</span>
          </div>
          <label>
            Job ID
            <div className="inline-control">
              <input value={jobId} onChange={(event) => setJobId(event.currentTarget.value)} required />
              <button className="secondary-button" type="button" onClick={() => setJobId(freshJobId())}>New</button>
            </div>
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
          <button className="primary-button" disabled={running || !host || !allowOncePermissions}>
            {running ? "Agent is running…" : "Start evaluation"}
          </button>
          <dl className="host-details">
            <div><dt>Artifact store</dt><dd title={host?.dataDir}>{host?.dataDir ?? "Unavailable"}</dd></div>
            <div><dt>MCP transport</dt><dd>stdio · bundled host</dd></div>
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
                    <p>{conciseMessage(event)}</p>
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
                        {articleQuality.diagnostics.wordCount} words
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
                {snapshot.artifacts.map((artifact) => (
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
