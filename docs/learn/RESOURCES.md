# Tauri v2 Resources

## Knowledge

- [Official Tauri v2 Documentation](https://v2.tauri.app)
  Comprehensive guide and API reference for Tauri v2. Use for: architecture, CLI reference, core features, and configuration.
- [Tauri v2 Calling Rust / Commands Guide](https://v2.tauri.app/develop/calling-rust/)
  Deep dive into `#[tauri::command]`, argument serialization, async commands, and error handling. Use for: frontend-to-backend IPC.
- [Tauri v2 Inter-Process Communication (IPC)](https://v2.tauri.app/concept/inter-process-communication/)
  Detailed architecture of how Tauri passes messages between Webview and Core safely. Use for: understanding events vs commands, performance, serialization.
- [Tauri v2 State Management Guide](https://v2.tauri.app/develop/state-management/)
  Patterns for `tauri::State`, `Mutex`, `RwLock`, and sharing long-lived backend state across commands. Use for: building persistent audio/runtime engine state.
- [Tauri v2 Capabilities & Permissions Security Model](https://v2.tauri.app/security/capabilities/)
  Documentation on the v2 capability system in `src-tauri/capabilities/`. Use for: configuring window permissions, IPC allowlists, and plugin security.
- [Tauri v2 Events System](https://v2.tauri.app/develop/events/)
  Guide for pub/sub messaging (`emit`, `listen`) across Rust and frontend. Use for: audio clock and timeline streaming.

## Wisdom (Communities)

- [Tauri Official Discord](https://discord.gg/tauri)
  Active community with dedicated channels for v2 help, Rust backend troubleshooting, and plugin discussions.
- [Tauri GitHub Discussions](https://github.com/tauri-apps/tauri/discussions)
  High-signal discussions on architecture, feature requests, and edge-case debugging.
- [r/tauri on Reddit](https://reddit.com/r/tauri)
  Showcase and community troubleshooting forum.
