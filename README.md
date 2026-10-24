# Code Orchestrate

A desktop app for launching and tracking coding Agent Sessions per Workspace. See
[`docs/spec/v1.md`](docs/spec/v1.md) for the v1 spec and [`GLOSSARY.md`](GLOSSARY.md) for the
domain vocabulary.

Built with Tauri 2 (Rust backend) and React + TypeScript (Vite, shadcn/ui). Rust owns all system
access; the webview only calls the app's commands and the folder picker dialog
([ADR 0001](docs/adr/0001-tauri-with-rust-owning-system-access.md)).

## Prerequisites

- [Rust](https://rustup.rs/) via rustup (stable)
- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) ("Desktop development with C++")
- [Node.js](https://nodejs.org/) 20+ with npm
- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 10/11)

## Commands

```sh
npm install          # install frontend dependencies
npm run tauri dev    # run the app with hot reload
npm run tauri build  # build an installer
npm test             # frontend tests (Vitest)
npm run test:rust    # backend tests (cargo test in src-tauri)
npm run typecheck    # TypeScript check
```

## Layout

- `src/`: React UI. `src/components/ui/` holds shadcn/ui components (owned in-repo); tests sit
  next to the code as `*.test.tsx`.
- `src-tauri/`: Rust backend (`src/lib.rs` builds the app). `capabilities/default.json` is the
  webview's whole permission set; `tests/` holds integration tests.
