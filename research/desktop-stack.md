# Research: desktop stack options

Question (from the ticket "Research: desktop stack options"): which desktop stack best fits Code Orchestrate v1? Compared: **Tauri 2 (TypeScript UI)**, **Electron (TypeScript UI)**, **Avalonia (.NET)**, **WinUI 3 (.NET)**.

This document recommends; the decision itself is made in the grilling ticket "Choose the desktop stack". Terms (Workspace, Agent, Session) follow `GLOSSARY.md`.

Researched 2026-10-23 against official docs. Each claim cites its source; claims marked *(judgement)* are the author's assessment, not a sourced fact.

## Recommendation

**Tauri 2 with a TypeScript UI**, with all process launching and transcript reading done on the Rust side.

- It is the only option that combines a web UI (the easiest route to a polished look and to xterm.js later) with a small native backend, no bundled browser, and real cross-platform reach.
- **Electron** is the safe runner-up: the most mature terminal-embedding path (node-pty + xterm.js, the VS Code stack) and the simplest single-language codebase, paid for with a bundled Chromium and a heavier footprint.
- **Avalonia** is the pick if the author would rather write C# than TypeScript + Rust: genuinely cross-platform and native-rendered, but embedding a terminal later has no first-party path.
- **WinUI 3** is ruled out by the cross-platform goal: it targets Windows only.

## Comparison

| Criterion | Tauri 2 (TS) | Electron (TS) | Avalonia (.NET) | WinUI 3 (.NET) |
|---|---|---|---|---|
| Polished UI, light/dark + override | Web UI; `setTheme(null)` follows OS, `"light"`/`"dark"` overrides [T3] | Web UI; `nativeTheme.themeSource = system/light/dark` [E1] | Fluent theme with Light/Dark; `RequestedThemeVariant` incl. `Default` [A1] | Fluent, the native Windows look [W1] |
| Launch `wt -d <dir> claude ...` | Rust `std::process::Command` or shell plugin (scoped permissions) [T2] | Node `child_process` in main process [E2] | `System.Diagnostics.Process` | Same as Avalonia |
| Embed a terminal later | xterm.js in webview + `portable-pty` (Rust) [P1] | xterm.js + `node-pty` (ConPTY) [N1] | No first-party terminal control *(gap)* | No first-party terminal control *(gap)* |
| Watch `~/.claude/projects` | `notify` crate [R1] | `fs.watch` / chokidar [F1] | `FileSystemWatcher` [D1] | `FileSystemWatcher` [D1] |
| Build-from-source toolchain | Rust (MSVC) + C++ Build Tools + Node [T1] | Node; C++ toolchain only once node-pty is added [N1] | .NET SDK only [A2] | .NET SDK + Windows App SDK, Windows only [W1] |
| Footprint | Uses system webview, no bundled browser [T4][T5] | Bundles Chromium + Node *(well known; largest)* | Own renderer, .NET runtime | .NET + Windows App SDK |
| Cross-platform | Windows, macOS, Linux [T1] | Windows, macOS, Linux | Windows, macOS, Linux [A2] | **Windows only** [W1] |

## Findings by criterion

### 1. Polished look and light/dark mode

- **Tauri**: `setTheme(theme?)` — "pass in `null` or `undefined` to follow system theme"; `Theme = "light" | "dark"`; `onThemeChanged` listens for system changes. On Linux and macOS the theme is app-wide, not per window [T3]. The UI itself is HTML/CSS, so `prefers-color-scheme` and any component library apply.
- **Electron**: `nativeTheme.themeSource` accepts `system`, `light`, `dark` (default `system`); setting `dark` also makes `prefers-color-scheme` match dark and darkens native menus and frames [E1]. This is exactly the "follow OS + manual override" model the map asks for.
- **Avalonia**: "Avalonia inherits the theme variant set by the system-wide user preference"; `FluentTheme` ships Light and Dark; `RequestedThemeVariant` overrides, and `Default` resets to the OS [A1]. Custom per-variant resources need `ThemeDictionaries` + `DynamicResource` [A1].
- **WinUI 3**: Fluent controls, the native Windows 11 look; Microsoft calls it "the recommended native UI framework for building new Windows desktop applications" [W1].
- *(judgement)* Both web stacks give the widest choice of polished components; Avalonia's Fluent theme looks professional out of the box; WinUI 3 looks most "native" on Windows only.

### 2. Launching external terminals

The launch itself is trivial in all four; the subtleties are in `wt` quoting, not in the stack.

- `wt` syntax: `wt [options] [command ; ]`; `new-tab` is the default command; `-d/--startingDirectory` sets the directory; the trailing `commandline` is "Executable with optional arguments"; `-w new|last|<name>` targets a window (`-w <name>` creates the window if absent) [WT1].
- `;` is `wt`'s command separator, so any argument containing `;` must be escaped or avoided; and launching `wt` *from PowerShell* blocks until the terminal closes, so spawn `wt.exe` directly from the app rather than via PowerShell [WT1].
- `wt` is an app execution alias in `%LOCALAPPDATA%\Microsoft\WindowsApps`; aliases can be disabled in Settings [WT1], so the configurable-terminal setting needs a fallback (e.g. `powershell.exe` / `cmd.exe /k`).
- **Tauri**: spawn from Rust (`std::process::Command`, or the shell plugin's `app_handle.shell().command(...)`). The shell plugin blocks dangerous commands by default and needs explicit capability scopes for JS-side use [T2]. *(judgement)* Keep launching in a Rust command so the webview never gets a general-purpose spawn permission.
- **Electron**: the main process has "all of Node.js APIs" [E2], so `child_process.spawn('wt.exe', [...], { detached: true })`.
- **.NET**: `Process.Start` with `ProcessStartInfo.ArgumentList`.
- macOS/Linux later: each stack spawns `open -a Terminal`, `gnome-terminal`, etc. equally; no stack advantage.

### 3. Path to an embedded terminal later

- **node-pty** (Microsoft, used by VS Code): supports Linux, macOS, Windows; Windows uses ConPTY and requires Windows 10 1809+; needs Electron 19+; is a native module requiring Python + C++ compiler + Windows SDK with Spectre-mitigated libs to build; "not thread safe" [N1].
- **portable-pty** (part of WezTerm): Rust "cross platform API for working with the psuedo terminal (pty) interfaces provided by the system", with runtime-selectable implementations, "important on Windows systems which have a variety of implementations"; API: `native_pty_system().openpty(...)`, `slave.spawn_command(...)`, `master.try_clone_reader()/take_writer()` [P1]. In Tauri, pipe its reader/writer to xterm.js over events/channels.
- **.NET**: no first-party PTY or terminal control from Microsoft or Avalonia *(gap: not found in official docs)*. Options would be P/Invoking ConPTY (`CreatePseudoConsole`) yourself, a community control, or hosting xterm.js in a WebView inside the native app, which reintroduces a web layer.
- *(judgement)* Electron has the most travelled path; Tauri is close behind and already ships the Rust toolchain that portable-pty needs; .NET is the weakest here.

### 4. Reading and watching many JSONL transcripts

- **Rust `notify`**: native backends (inotify on Linux, FSEvents by default on macOS), `RecursiveMode::Recursive`, and `PollWatcher` fallback. Caveats: Linux inotify limits count every watched file/folder in recursive watches; events may be dropped on large directories; network filesystems may emit nothing [R1].
- **.NET `FileSystemWatcher`**: `IncludeSubdirectories`; the Windows buffer "can overflow", after which it "will only provide blanket notification"; the `Error` event fires on overflow; larger `InternalBufferSize` costs non-paged memory [D1].
- **Node**: `fs.watch` with `recursive` (Windows/macOS long-standing; Linux added in newer Node) [F1, not fully verified — page too long to read in full]; most Electron apps use chokidar on top.
- *(judgement)* All watchers are lossy under bursts, so the design should treat events as "something changed under this directory" hints and re-scan file sizes/mtimes, reading JSONL incrementally from a stored byte offset. That design is stack-neutral. Rust and .NET parse large JSONL off the UI thread with less ceremony than Node (where heavy parsing in the Electron main process can stall IPC; a `UtilityProcess` helps [E2]).

### 5. Build-from-source friction and footprint

- **Tauri** on Windows: Microsoft C++ Build Tools ("Desktop development with C++"), Rust via rustup with the MSVC host triple, WebView2 (preinstalled on Windows 10 1803+ / Windows 11), plus Node for the frontend [T1]. Linux needs `libwebkit2gtk-4.1-dev` and friends; macOS needs Xcode CLT [T1]. Largest *toolchain* (multi-GB C++ Build Tools), but `npm run tauri build` is one command afterwards.
- Tauri does not bundle a browser: WebView2 on Windows (self-updating Chromium), WKWebView on macOS, WebKitGTK on Linux [T5]. Docs call its binaries "very small by default" without giving a number [T4]. Trade-off: three rendering engines, so the UI must be tested on WebKit too [T5].
- **Electron**: only Node is needed until a native module (node-pty) arrives, which then pulls in Python + C++ + Windows SDK [N1]. Ships Chromium + Node in every build *(well known; largest binary and memory footprint of the four)*.
- **Avalonia**: .NET SDK only (minimum .NET 8 for desktop) [A2]; `dotnet run` is one command; can cross-compile macOS apps from Windows/Linux [A2]. Smallest toolchain for a cloner.
- **WinUI 3**: .NET + Windows App SDK; Windows 10 1809+ [W1]. Windows-only build.
- No measured binary/RAM numbers were taken here *(gap)*; a throwaway prototype per finalist would settle it if footprint matters to the decision.

### 6. Cross-platform viability without a rewrite

- Tauri: Windows, macOS, Linux from one codebase [T1][T5].
- Electron: Windows, macOS, Linux.
- Avalonia: Tier 1 on Windows 11 24H2, macOS 26, Ubuntu 25 / Fedora 43 / Debian 13; "uses the Win32 API directly on Windows" [A2].
- WinUI 3: "run on Windows 10, version 1809 (build 17763) and later, including Windows 11" [W1]; no macOS/Linux, so it fails this criterion.

## Trade-offs to grill in "Choose the desktop stack"

1. **Two languages (TS + Rust) vs one (TS, or C#).** Tauri's main cost. If the author is not comfortable in Rust, Electron or Avalonia keep it to one language.
2. **Footprint vs maturity.** Tauri is lean; Electron is the proven path for embedded terminals (VS Code).
3. **Web UI vs native UI.** Web gives the most design freedom and makes xterm.js trivial; Avalonia gives a consistent self-rendered look with no webview differences across OSes.
4. **Build friction for cloners.** Avalonia (.NET SDK only) < Electron (Node; C++ once node-pty lands) < Tauri (Rust + C++ Build Tools + Node).

## Gaps and open questions

- No measured binary size or idle RAM; recommend a throwaway "hello window + launch wt" prototype for the finalists if footprint is decisive.
- No first-party terminal control for .NET found; if Avalonia is chosen, embedding needs its own research.
- Node `fs.watch` recursive support on Linux quoted from memory of the docs, not verified line by line.

## Sources

- [T1] Tauri prerequisites — https://v2.tauri.app/start/prerequisites/
- [T2] Tauri shell plugin — https://v2.tauri.app/plugin/shell/
- [T3] Tauri JS window API (`setTheme`, `theme`, `onThemeChanged`) — https://v2.tauri.app/reference/javascript/api/namespacewindow/
- [T4] Tauri app size — https://v2.tauri.app/concept/size/
- [T5] Tauri webview versions — https://v2.tauri.app/reference/webview-versions/
- [E1] Electron `nativeTheme` — https://www.electronjs.org/docs/latest/api/native-theme
- [E2] Electron process model — https://www.electronjs.org/docs/latest/tutorial/process-model
- [N1] node-pty README — https://github.com/microsoft/node-pty
- [P1] portable-pty docs — https://docs.rs/portable-pty/latest/portable_pty/
- [R1] notify crate docs — https://docs.rs/notify/latest/notify/
- [F1] Node.js `fs.watch` — https://nodejs.org/api/fs.html#fswatchfilename-options-listener
- [D1] .NET `FileSystemWatcher` — https://learn.microsoft.com/en-us/dotnet/api/system.io.filesystemwatcher
- [A1] Avalonia theme variants — https://docs.avaloniaui.net/docs/guides/styles-and-resources/how-to-use-theme-variants
- [A2] Avalonia supported platforms — https://docs.avaloniaui.net/docs/overview/supported-platforms
- [W1] WinUI 3 overview — https://learn.microsoft.com/en-us/windows/apps/winui/winui3/
- [WT1] Windows Terminal command line arguments — https://learn.microsoft.com/en-us/windows/terminal/command-line-arguments
