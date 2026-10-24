# Tauri 2 + React, with Rust owning all system access

Code Orchestrate is a Tauri 2 desktop app with a React + TypeScript UI (shadcn/ui), chosen over Electron and Avalonia because it pairs a polished web UI and light/dark theming with a small footprint, cross-platform reach, and a credible path to embedding a terminal later (xterm.js + `portable-pty`). All system access — launching Agents in terminals, reading and watching transcripts, persistence — lives in Rust commands; the webview gets no shell or filesystem capability, so the UI stays a pure view and this boundary is where the Agent seam sits.

## Considered Options

- **Electron**: single language and the most proven embedded-terminal path (node-pty, as in VS Code), rejected for bundling Chromium and the heavier footprint.
- **Avalonia (.NET)**: simplest build (.NET SDK only) and a good native Fluent look, rejected because there is no first-party way to embed a terminal later.
- **WinUI 3**: Windows-only, which conflicts with the cross-platform goal.

## Consequences

- Two languages in the repo; anyone building from source needs Rust (rustup), the C++ Build Tools, and Node.
- The webview differs per OS (WebView2 / WKWebView / WebKitGTK), so the UI must be checked on WebKit once macOS/Linux are supported.
