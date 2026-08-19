# Digest — Setup & Getting Started

> A teaching guide: the *why* behind each step, so you understand what you are doing rather than just running commands. We are setting up only the workspace and the minimal dependencies Feature 1 needs — nothing more. The rest lands feature-by-feature per the [master roadmap](milestones.md).

---

## The big idea: you already have a "workspace"

Before running any command, understand the shape of what you are building.

A Tauri application is **two sides in one repository**:

- `src-tauri/` — the **Rust core**. This is the durable-truth side: it owns state, SQLite persistence, filesystem artifacts, process supervision, ACP agent communication, and audio pipeline orchestration.
- `src/` — the **web frontend**. This is the presentation side: React 19, Tailwind styling, interactive visual components, and deterministic playback execution.

These two sides share **nothing at runtime except typed IPC** through the Tauri bridge. They do not share memory, they do not share state, and they do not share a language. The seam between them is formalized in [contracts.md](contracts.md).

Why does this split matter? Because it keeps responsibilities cleanly separated:
- Rust tracks job durability, file I/O, and external process lifecycles.
- React renders what the pre-compiled lesson timeline dictates and responds to user interactions.

The whole project architecture hangs off this invariant (see [System Architecture](architecture.md) — "The agent plans and compiles; Rust owns durable storage; React owns presentation and deterministic playback").

---

## Step 1 — Prerequisites, and why each one

Three things are required on your host system:

| Prerequisite | Why you need it |
| :--- | :--- |
| **Rust** (via rustup) | `rustc` + `cargo` compile and run the native desktop backend. Tauri requires the stable toolchain. |
| **Node.js LTS (20+) + pnpm** | Node runs the frontend tooling (Vite). pnpm is this project's package manager — see [agents.md](../agents.md). |
| **Tauri system dependencies** | Platform-specific native libraries (Linux: `webkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `build-essential`). These are required for the **OS webview** to render the native application window. See <https://v2.tauri.app/start/prerequisites>. |

Verify before proceeding:
```bash
rustc --version
cargo --version
node --version
pnpm --version
```

---

## Step 2 — Project Structure & Scaffold

The repository is structured as a standard Tauri v2 project:

```
digest/
|-- src/                         # React frontend
|-- src-tauri/                   # Rust backend
|-- docs/                        # Project documentation & specs
|-- package.json                 # Frontend dependencies & scripts
`-- vite.config.ts               # Vite bundler config
```

---

## Step 3 — Read what you got

Before adding dependencies, understand the key generated files and **how they map to your milestones**:

| File | What it is | What it maps to |
| :--- | :--- | :--- |
| `src-tauri/src/lib.rs` | Where `#[tauri::command]` handlers live, registered by `invoke_handler!` | **Feature 1** — the IPC starting point |
| `src-tauri/capabilities/default.json` | Tauri's permission allowlist | **D-001** — the security model: permissions are closed by default |
| `src-tauri/tauri.conf.json` | Window configuration, app identifier, and bundler settings | **Feature 1** — desktop shell configuration |
| `src/services/api.ts` (once created) | The frontend IPC wrapper with mock fallback support | **Feature 1 / Feature 9** — unlocks browser-only testing |

The key insight: `capabilities/default.json` having restricted permissions by default is **not a bug, it is the core security feature**. It is the reason we chose Tauri over Electron (decision [D-001](decisions.md#d-001--desktop-framework-tauri-v2)) — local article data and filesystem access are protected by explicit capability grants.

---

## Step 4 — Minimal dependencies (the "not everything at once" part)

Only install what Feature 1 needs — a working window, typed IPC, and styled layout:

```bash
# Frontend — Tailwind CSS for layout styling
pnpm add tailwindcss @tailwindcss/vite

# Rust — async runtime, serialization, typed errors, and shell capabilities
cd src-tauri
cargo add tokio --features rt-multi-thread,macros,sync
cargo add serde --features derive
cargo add serde_json
cargo add thiserror
```

**Why each one is here now:**

| Dependency | Why it's needed now |
| :--- | :--- |
| `tokio` | The async runtime — needed for `spawn_blocking` (so CPU-heavy jobs never stall the IPC loop) and `mpsc` channels for the database actor. |
| `serde` + `serde_json` | The serialization layer — Rust structs cross the IPC boundary as JSON. |
| `thiserror` | The typed `AppError` enum — structured errors that the frontend can inspect and display. |
| `tailwindcss` | Modern CSS-first utility styling for responsive shell layouts. |

**What NOT to add yet, and why:**

| Dependency | Arrives with | Why not now |
| :--- | :--- | :--- |
| `rusqlite` (WAL persistence) | Feature 3 | No database logic exists yet — write the schema and actor design first. |
| `reqwest` (HTTP client) | Feature 2 | No article fetching logic yet. |
| `@mermaid-js/mermaid` | Feature 11 | No diagrams to render until the presentation engine is ready. |
| `shiki` (syntax highlighter) | Feature 11 | No code walkthrough component to render yet. |

Trust the [master roadmap](milestones.md). Adding dependencies before their corresponding feature accumulates dead weight and security overhead.

---

## Step 5 — Verify (the Feature 1 smoke test)

```bash
# Install frontend dependencies
pnpm install

# Run desktop application in development mode
pnpm tauri dev
```

Then run the Feature 1 smoke test:
1. Ensure `ping` command is registered in `src-tauri/src/lib.rs`.
2. Call `invoke('ping')` from React in `src/App.tsx`.
3. Check the webview console to confirm `{ message: "pong", version: "0.1.0" }` is returned.

This proves the **entire seam works**: Webview -> IPC bridge -> Rust command handler -> Result back to Webview. Once this round-trip works, every downstream feature is simply adding structured commands and events along this seam.

Also confirm Rust compiles cleanly:
```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

---

## Exercises to learn the codebase

1. **Trace the IPC path.** Read `src-tauri/src/lib.rs` and follow how `#[tauri::command]` exposes a Rust function to JavaScript's `invoke()` call.
2. **Read `capabilities/default.json`.** Notice how network and filesystem permissions must be explicitly permitted.
3. **Add a typed command.** Implement `fn get_system_status() -> Result<SystemStatus, AppError>`, wire it through `generate_handler!`, and call it with TypeScript types in React.

---

## Where to go next

- [Master Roadmap](milestones.md) — Feature 1 is the first milestone to complete.
- [Shared Contracts](contracts.md) — Formal definitions of all structs and interfaces crossing the IPC bridge.
- [Decision Log](decisions.md) — Why each architectural choice was made.
