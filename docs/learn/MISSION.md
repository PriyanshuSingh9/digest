# Mission: Mastering Tauri v2 for the Digest AI Engine

## Why
Build and ship the **Digest** desktop application (a local-first AI article learning and synchronized presentation engine) by mastering Tauri v2 architecture, Rust-to-frontend IPC, state management, event streaming, and local platform capabilities.

## Success looks like
- Understand the Tauri v2 process model (Webview vs. Rust core) and security boundary.
- Implement custom Rust commands (`#[tauri::command]`) with type-safe argument passing and error handling.
- Stream real-time timeline/audio sync events between Rust and the React frontend using Tauri events (`emit` / `listen`).
- Manage application state in Rust (`tauri::State`, thread-safe concurrency with `Mutex`/`RwLock`/channels) and expose it cleanly to React.
- Configure Tauri v2 capabilities and permissions to safely handle local filesystem and process interactions.
- Independently build, debug, and extend features in the `digest` codebase.

## Constraints
- **Stack**: Tauri v2, Rust backend, React 19 + TypeScript + Vite frontend.
- **Platform**: Linux (WebKitGTK) with cross-platform desktop target.
- **Focus**: Practical, hands-on understanding directly applicable to the `digest` project and its PRD.

## Out of scope
- Deep Rust macro compiler internals.
- Mobile-specific deployment (iOS/Android) for the initial desktop phase.
- Premature GUI styling optimizations before IPC and core architecture are mastered.
