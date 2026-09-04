# Research: Agent Integration, Article Capture, and Web Runtime

- **Status:** Planning input, not yet an accepted architecture decision
- **Date:** 2026-08-13
- **Authority:** `article_learning_engine_prd.md` is the product and architecture source of truth. Other repository documents are stale and are not inputs to this recommendation.
**Scope:** Clarify three parts of the product specification before implementation:

1. what an "ACP-connected agent" means,
2. what complete article ingestion must preserve,
3. whether the playback runtime should use Astro with React islands or a React application.

This note uses primary documentation and source code. The inspected reference repositories are available locally under `.delta/research/` and are intentionally not part of the project.

## Executive recommendation

1. Treat OpenCode and `agy` as the two V1 **agent providers**. Defer Grok Build, Codex, Claude Code, and other integrations.
2. Make the Rust host's stable boundary a provider-neutral `AgentDriver` with explicit capabilities and canonical events, but implement only the capabilities needed by OpenCode and `agy` in V1. Implement native ACP as the preferred driver substrate; use a thin adapter where an agent does not expose ACP directly.
3. Expose Digest's deterministic tools to agents as an MCP server. ACP is the client-to-agent control channel; MCP is the agent-to-tool channel.
4. Replace "fetch and extract readable article content" with a layered **capture, extract, localize, and validate** process. Preserve an immutable raw capture plus an ordered normalized document containing text, code, tables, links, equations, and media.
5. Keep the current React + TypeScript + Vite direction for the application and lesson player. Astro islands do not materially simplify a player whose audio clock coordinates most of the visible page. Reconsider Astro for a separate mostly-static publishing surface, not for the core application.
6. Use Tauri as the V1 Linux evaluation client for testing and improving generated lesson quality. Keep the engine, agent tools, artifacts, and player independent of Tauri.
7. Target Hermes Agent as the V2 remote control plane. Hermes must be able to reuse the same MCP tools and execution service without preserving the Tauri host.
8. Keep compilation and playback as separate planes. Compilation needs an agent and private tools; playback needs only a browser, a manifest, and assets.

## 1. Agent integration

### 1.1 V1 provider scope

V1 supports:

1. OpenCode, preferably through its native `opencode acp` server.
2. Google Antigravity CLI (`agy`) through an ACP adapter unless an official native ACP mode becomes available.

Codex, Claude Code, Grok Build, and other agents are deferred. The provider seam should avoid preventing those integrations, but V1 must not implement speculative provider behavior or UI for them.

Hermes Agent is a V2 integration rather than a V1 provider. V1 uses the Tauri client with OpenCode and `agy` to evaluate output quality. V2 moves orchestration to Hermes on a remote Linux server while retaining the V1 engine, tools, artifacts, manifests, and player.

### 1.2 ACP is a control protocol, not a universal provider implementation

ACP defines communication between a client application and an agent. Its normal topology is a client spawning an agent subprocess and communicating over JSON-RPC on stdin/stdout. A connection can contain multiple sessions, and updates stream back as notifications. The stable protocol requires capability negotiation during initialization before optional operations such as session loading or resumption are used.

Sources:

- [ACP architecture](https://agentclientprotocol.com/get-started/architecture.md)
- [ACP v1 session setup and capability checks](https://agentclientprotocol.com/protocol/v1/session-setup.md)
- [ACP implementation list](https://agentclientprotocol.com/get-started/agents.md)
- [ACP repository and versioning](https://github.com/agentclientprotocol/agent-client-protocol)

The PRD should therefore define an ACP-connected agent as:

> An agent provider reached either through a native ACP server or through a provider adapter that projects the provider's native session, event, approval, and cancellation behavior into Digest's agent-driver contract.

For V1, this definition covers OpenCode and `agy` without claiming that both upstream CLIs expose identical protocol behavior.

### 1.3 What T3 Code demonstrates

T3 Code is useful as a reference because it controls several local agent products from web, desktop, and mobile surfaces. Its server does not assume that one protocol erases all provider differences.

At inspected commit [`c7c1dfe`](https://github.com/pingdotgg/t3code/tree/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44):

- [`ProviderAdapterShape`](https://github.com/pingdotgg/t3code/blob/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44/apps/server/src/provider/Services/ProviderAdapter.ts) defines canonical operations for sessions, turns, interruption, approvals, structured user input, rollback, shutdown, and runtime events.
- [`ProviderDriver`](https://github.com/pingdotgg/t3code/blob/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44/apps/server/src/provider/ProviderDriver.ts) separates static driver metadata and typed configuration from each live provider instance. Per-instance process state is scoped and independently released.
- [`ProviderService`](https://github.com/pingdotgg/t3code/blob/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44/apps/server/src/provider/Services/ProviderService.ts) is the cross-provider facade used by the transport layer.
- [`OpenCodeAdapter`](https://github.com/pingdotgg/t3code/blob/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44/apps/server/src/provider/Layers/OpenCodeAdapter.ts) uses OpenCode's SDK and translates native events, permissions, questions, cancellation, and session state into the canonical provider model.
- [`AntigravityProvider`](https://github.com/pingdotgg/t3code/blob/c7c1dfe4df99edf65a49d8a31b39ef1361f37f44/apps/server/src/provider/Layers/AntigravityProvider.ts) consumes ACP types and discovers models, commands, authentication state, and capabilities rather than hard-coding parity with other providers.

The reusable lesson is the shape of the seam, not T3 Code's Node/Effect implementation:

```text
UI / job coordinator
        |
        v
AgentService
        |
        v
AgentDriver + capabilities
        |
        +-- NativeAcpDriver
        +-- OpenCodeDriver or OpenCode ACP process
        +-- Codex ACP adapter
        +-- Agy ACP adapter
        +-- Claude driver/adapter
        +-- Grok driver/adapter
```

Provider identity, model identity, authentication, capabilities, and session identity must remain separate. A single enum plus `start_session`, `send_message`, and `cancel` is too shallow for approvals, elicitation, resume behavior, model selection, tool events, and provider-specific lifecycle failures.

### 1.4 What the `agy` adapter demonstrates

The inspected `agy-acp` implementation at commit [`b98dbc8`](https://github.com/shindgew/agy-acp/tree/b98dbc839adaea5754826da1b36edb3174665734) is a compatibility adapter around the Antigravity CLI. Its [documented architecture](https://github.com/shindgew/agy-acp/blob/b98dbc839adaea5754826da1b36edb3174665734/README.md) keeps one interactive `agy` PTY per ACP session, persists bindings for restart recovery, and translates structured conversation records into ACP updates.

This validates two requirements for Digest:

- provider adapters need supervised process lifecycle and explicit recovery semantics;
- the durable Digest job and artifact record cannot depend on the provider's in-memory session.

Third-party adapters can also carry upstream terms-of-service risk. They should be optional integrations, with native or officially supported ACP modes preferred when available.

### 1.5 ACP in front, MCP behind

The current PRD calls Digest operations "agent tools" but does not define how an external agent receives them. ACP's own architecture says clients commonly pass MCP server configuration when creating a session. The agent then connects to those MCP servers. ACP is in front of the agent; MCP is behind it.

Sources:

- [ACP architecture: MCP](https://agentclientprotocol.com/get-started/architecture.md#mcp)
- [ACP v1 session setup: MCP servers](https://agentclientprotocol.com/protocol/v1/session-setup.md#mcp-servers)
- [MCP-over-ACP RFD](https://agentclientprotocol.com/rfds/mcp-over-acp.md)

For V1, the most interoperable arrangement is:

```text
Digest host -- ACP/stdin+stdout --> agent
      |
      +-- starts digest-tools MCP server
                   ^
                   |
             MCP/stdin+stdout
                   |
                 agent
```

MCP-over-ACP is promising but is an RFD and existing agents do not all support it. Start with the required MCP stdio transport. Keep the tool implementation behind a transport-independent Rust service so MCP-over-ACP can be added later.

### 1.6 Proposed agent contract

The exact Rust API should follow the selected ACP SDK, but the domain boundary needs at least:

```text
AgentDriver
  probe
  initialize
  authenticate/logout when supported
  start_session
  load_or_resume_session when supported
  prompt
  cancel
  close
  respond_to_permission
  respond_to_elicitation
  stream_events
  capabilities

AgentCapabilities
  session_load
  session_resume
  session_close
  prompt_images
  mcp_stdio
  mcp_http
  elicitation
  permission_requests
  model_configuration
```

The canonical event model should preserve provider-native data in an extension metadata field while giving the application stable events for messages, plans, tool calls, permissions, artifacts, usage, completion, cancellation, and failure.

Use ACP stable v1 for V1. ACP v2 is currently documented as draft, so v2 support should be negotiated rather than assumed.

## 2. Complete article capture and extraction

### 2.1 Readability is only one stage

Mozilla Readability, the implementation used by Firefox Reader View, extracts article HTML, plain text, title, byline, excerpt, site name, language, publication time, and related metadata. It can preserve article HTML for further processing, but it is not a complete archival or media-understanding system.

Source: [Mozilla Readability README and API](https://github.com/mozilla/readability)

Trafilatura demonstrates a broader extraction target: its structured extraction can include tables, formatting, links, images and metadata, and it falls back between extraction algorithms when the initial result is too short.

Sources:

- [Trafilatura extraction functions](https://trafilatura.readthedocs.io/en/latest/usage-python.html#extraction-functions)
- [Trafilatura element selection](https://trafilatura.readthedocs.io/en/latest/usage-python.html#choice-of-html-elements)

SingleFile demonstrates the distinction between readable extraction and faithful capture. It can preserve a complete page with images, stylesheets, fonts, frames, and deferred resources for offline viewing.

Sources:

- [SingleFile project](https://github.com/gildas-lormeau/singlefile)
- [SingleFile CLI architecture](https://github.com/gildas-lormeau/single-file-cli)

Digest should not use a single output for both source preservation and lesson input.

### 2.2 Proposed layered ingestion

```text
URL
 |
 +-- 1. Safe fetch
 |      response bytes, headers, redirects, final URL, timestamps
 |
 +-- 2. Raw source snapshot
 |      immutable original HTML and fetch metadata
 |
 +-- 3. Readable extraction
 |      candidate article DOM plus extraction diagnostics
 |
 +-- 4. Structural normalization
 |      ordered blocks: headings, prose, code, lists, quotes,
 |      tables, equations, links, embeds, and media
 |
 +-- 5. Asset localization
 |      resolve URLs, select image candidates, download permitted assets,
 |      hash bytes, inspect MIME/dimensions, store local references
 |
 +-- 6. Completeness validation
 |      block counts, text coverage, media failures, truncation signals,
 |      extraction confidence, and optional browser-render fallback
 |
 +-- 7. Agent understanding
        concepts, OCR/captions when needed, educational interpretation
```

Use a fast HTTP fetch first. Escalate to a supervised headless browser only when the static response is an application shell, required content is absent, or extraction quality is below a defined threshold. Browser rendering must not be the default because it costs substantially more CPU and memory and expands the security surface.

V1 targets Chromium and Chromium-based webviews. The extraction path does not need Firefox compatibility and should not select Mozilla technology merely for parity with Firefox Reader View.

Prefer extracting from Chromium's rendered DOM when browser rendering is required. For the normal fast HTTP path, choose a Rust-native extractor based on measured structural and media coverage. A checked-in fixture corpus of real technical articles should determine whether a single Rust extractor is sufficient; Mozilla Readability and Trafilatura are not required V1 dependencies.

### 2.3 Media is part of the canonical source

An image is not just a URL. The normalized representation should retain:

```text
id
source block position
original URL
resolved URL
local artifact reference when captured
content hash
declared and detected MIME type
width and height when known
alt text
caption and nearby figure text
credit/attribution when present
source-set candidates
load/capture status
```

The same principle applies to equations, diagrams, audio/video embeds, and downloadable code/data files. V1 can limit which media it downloads, but it must preserve the ordered reference and capture status rather than silently dropping unsupported content.

Agent-generated OCR, image descriptions, diagram interpretations, and inferred relationships are derived artifacts. They must not overwrite author-provided alt text or captions and must use the same provenance classifications as generated prose.

### 2.4 Ingestion safety requirements

The fetcher and asset downloader need explicit controls for:

- SSRF protection, including redirects to loopback, link-local, or private addresses;
- allowed HTTP/HTTPS schemes;
- redirect, response-size, decompression, and timeout limits;
- per-article asset count and byte budgets;
- MIME sniffing rather than trusting filename extensions;
- SVG and HTML sanitization before browser rendering;
- content-security policy for archived or generated content;
- no paywall or access-control bypass;
- recorded partial failures rather than silent omission.

Raw archived HTML must never be inserted into the playback DOM as trusted application HTML.

## 3. Web runtime: Astro versus React + Vite

### 3.1 What Astro islands optimize

Astro renders pages to static HTML by default and hydrates independent interactive components only when requested. This is a strong fit when most of a page is static and interactivity is isolated.

Sources:

- [Astro islands architecture](https://docs.astro.build/en/concepts/islands/)
- [React and other framework components in Astro](https://docs.astro.build/en/guides/framework-components/)
- [Astro static and on-demand rendering](https://docs.astro.build/en/guides/on-demand-rendering/)

### 3.2 Why the lesson player is not naturally a set of islands

The lesson page has one authoritative audio clock. Playback controls, transcript highlighting, current visual, navigation, seeking, progress persistence, and transitions all react to that shared state. Splitting them into independently hydrated islands adds cross-island coordination and duplicated hydration boundaries without reducing the JavaScript needed for the core experience.

The honest Astro design would therefore be:

```astro
<StaticLessonMetadata />
<LessonPlayer client:load />
```

Most of the product is still one React application island. Astro would add a build and routing layer while providing little runtime benefit.

### 3.3 Recommended V1 runtime

Use Tauri as the V1 evaluation host around a single reusable React + TypeScript player package built with Vite:

```text
Tauri on Linux
  +-- starts and observes OpenCode or agy ACP sessions
  +-- invokes the same Rust application services exposed through MCP
  +-- manages local evaluation workflows
  +-- hosts the React lesson library and player

Rust HTTP service
  +-- serves versioned web runtime assets
  +-- serves lesson API/manifests
  +-- serves content-addressed media with range requests
```

Tauri is an evaluation client, not the permanent control plane. Tauri commands must delegate to shell-independent Rust application services; MCP tool handlers must delegate to those same services. Agent, engine, storage, and manifest types must not depend on Tauri types.

The player should be transport-independent: it receives a manifest URL or manifest object and an asset resolver. The same build can run in Tauri, on localhost, through a tunnel, or behind the V2 remote Rust service.

Any-device access depends more on responsive design, media compatibility, HTTP range support, secure remote access, and a stable manifest than on Astro. `HTMLMediaElement.currentTime` is widely available and is the appropriate playback clock; it is an approximation updated by the media pipeline, so rendering should sample it during animation frames rather than use an independent timer.

Source: [MDN `HTMLMediaElement.currentTime`](https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement/currentTime)

### 3.4 When Astro becomes the better choice

Reconsider Astro for a separate publishing surface if Digest later needs:

- mostly-static public lesson landing pages;
- crawlable reading-mode pages;
- a marketing/documentation site;
- build-time exports where each lesson becomes immutable HTML;
- isolated optional interactions rather than a synchronized application.

That publishing surface can embed the same React player package as one island. It should not force the local application or Rust headless server to require an Astro SSR/Node runtime.

## 4. Remote access and deployment boundary

Cloudflare Tunnel can publish a local HTTP application without opening inbound ports. Cloudflare Access can put authentication in front of a self-hosted application and is deny-by-default once policies are configured.

Remote deployment is a V2 concern. The intended migration is:

```text
V1: Tauri + OpenCode/agy -> Digest MCP tools -> local Rust services
V2: Hermes remote gateway -> Digest MCP tools -> remote Rust services
```

The MCP contracts and Rust application services are the migration seam. V2 replaces the actor and host topology, not the compiler or lesson format.

Sources:

- [Cloudflare private web application architecture](https://developers.cloudflare.com/cloudflare-one/setup/secure-private-apps/private-web-app/)
- [Cloudflare Access for a self-hosted application](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/self-hosted-public-app/)

Recommended defaults:

- bind the Digest server to loopback unless the user explicitly enables LAN access;
- treat quick tunnels as development-only;
- require a named tunnel plus Cloudflare Access for persistent Internet access;
- keep the agent, MCP tools, and administrative mutation endpoints unavailable from the public playback origin;
- split read-only playback routes from authenticated compilation/control routes;
- do not put secrets in manifests or browser bundles.

## 5. PRD clarifications to make

The PRD is authoritative. Stale repository documents must not be used to reinterpret its rule that the agent is the workflow orchestrator.

The PRD should nevertheless make the operational boundary explicit:

> The agent owns semantic planning and adaptive sequencing. The durable job coordinator owns lifecycle, policy, budgets, cancellation, retries explicitly requested by policy or the agent, artifact commits, and recovery. It does not invent semantic stages or silently continue semantic work after the agent has stopped.

This preserves the PRD's agent authority without making an agent session responsible for durable state or process supervision.

Other PRD changes implied by this research:

1. Rename the ACP section to "Agent Provider Integration", define OpenCode and `agy` as the V1 providers, and describe native ACP plus adapters.
2. Add MCP as the explicit transport for Digest tools.
3. Replace the illustrative three-method `AgentProvider` trait with capability negotiation and a canonical event model.
4. Expand ingestion requirements and the canonical article schema for media and capture diagnostics.
5. Separate raw source capture, normalized source, and generated media understanding.
6. Keep React + Vite inside the V1 Tauri evaluation client and record Astro publishing as a revisit condition.
7. Define Tauri as a replaceable V1 evaluation host and Hermes as the V2 remote control plane.
8. Add authenticated V2 remote-access requirements and isolate playback from compilation/control endpoints.

## 6. Proposed validation spikes before implementation

1. **Agent spike:** connect OpenCode through native ACP and `agy` through an ACP adapter to a minimal Rust client; pass a two-tool Digest MCP server; verify prompt, tool call, permission, cancellation, and session resume behavior.
2. **Extraction corpus:** check in 20 representative technical article fixtures covering static blogs, documentation, code-heavy pages, tables, math, lazy images, `srcset`, and client-rendered pages. Measure Rust-native static extraction and Chromium rendered-DOM fallback for structural and media coverage.
3. **Playback spike:** play one 30-minute lesson in the Linux Tauri WebKitGTK webview, desktop Chromium, and Chrome Android; verify seeking, background/resume behavior, range requests, responsive layout, and progress persistence.
4. **V2 deployment spike:** run Hermes and the Digest execution service on a remote Linux host, connect Hermes to the same Digest MCP tools used in V1, and expose only read-only playback through a named Cloudflare Tunnel protected by Access.

These spikes should produce measured acceptance criteria for the implementation roadmap rather than leaving framework and provider behavior as assumptions.
