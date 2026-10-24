# Research: Claude Code transcripts and launch flags

Question (from the ticket "Research: Claude Code transcripts and launch flags"): what exactly does Claude Code record on disk, and how do its CLI launch flags behave, so Code Orchestrate can discover Workspaces, list Sessions, compute stats, and launch/resume reliably?

Date: 2026-10-23. Claude Code installed locally: **2.1.310** (native `claude.exe` at `%USERPROFILE%\.local\bin`). Terms (Workspace, Agent, Session) follow `GLOSSARY.md`.

## Sources

- [D1] Claude Code docs, *Manage sessions*: https://code.claude.com/docs/en/sessions (sections "Where transcripts are stored", "Where the session picker looks", "Resume by session ID or name", "Name your sessions", "Branch a session")
- [D2] Claude Code docs, *CLI reference*: https://code.claude.com/docs/en/cli-reference (rows for `-c/--continue`, `-r/--resume`, `--fork-session`, `--session-id`, `-n/--name`, `claude agents`)
- [D3] Claude Code docs, *Settings files and precedence*: https://code.claude.com/docs/en/settings (`~/.claude` on Windows, `CLAUDE_CONFIG_DIR`)
- [D4] Microsoft Learn, *Windows Terminal command line arguments*: https://learn.microsoft.com/en-us/windows/terminal/command-line-arguments
- [L] Read-only inspection of the local transcripts under `C:\Users\miris\.claude\projects\` (7 project folders, 11 top-level transcripts, 25 subagent transcripts, versions 2.1.156 to 2.1.310), plus `~/.claude/history.jsonl`, `~/.claude/sessions/*.json`, `~/.claude/stats-cache.json`. Only field names, shapes and counts were extracted; no conversation content.

Anything marked **[L]** is observed behaviour, not documented contract.

## 1. Format stability (read this first)

The docs say outright: "The entry format is internal to Claude Code and changes between versions, so scripts that parse these files directly can break on any release. To build on session data, use `/export` or the script interfaces instead." [D1]

What is documented (and so reasonably stable): the storage location, the file naming `<session-id>.jsonl`, the folder-name rule, retention, the env vars that move storage, and the CLI flags. Everything about record *contents* below is [L]. The app's parser should be defensive: skip unknown `type`s, tolerate missing fields, and never fail a whole scan because one line is bad.

Evidence of churn [L]: transcripts from 2.1.156 (desktop app entrypoint) have 7 record types; transcripts from 2.1.30x have up to 17 (`mode`, `permission-mode`, `atis-latch`, `api-request*`, `cost-state`, `pr-link`, `dev-mods`, ... are newer).

## 2. Storage layout

Root: `~/.claude/projects/` (on Windows `%USERPROFILE%\.claude\projects\`), relocatable with `CLAUDE_CONFIG_DIR` [D1, D3]. Per-run override `CLAUDE_CODE_PROJECT_DIR_NAME` (only honoured with `CLAUDE_CONFIG_DIR` set) [D1]. The app should resolve the root as `$CLAUDE_CONFIG_DIR/projects` if set, else `~/.claude/projects`, and make it a setting.

```
projects/
  <encoded-project>/
    <session-id>.jsonl                 # main transcript of one Claude Code session ID
    <session-id>/                      # optional sidecar dir
      subagents/agent-<agentId>.jsonl  # subagent transcripts (isSidechain: true)
      subagents/agent-<agentId>.meta.json   # {agentType, description, toolUseId}
      subagents/*.forked-skill*.json   # seen for forked skills
      tool-results/*.txt               # large tool outputs spilled to disk
    memory/                            # auto memory (MEMORY.md + notes), not sessions
```

[L] One project folder (`D--Programming-Textual`) contained only `memory/` and no transcripts, so "folder exists" does not imply "has Sessions" (transcripts age out, memory does not).

### 2.1 Folder-name encoding (lossy)

Rule: "`<project>` is your working directory path with non-alphanumeric characters replaced by `-`. For a working directory whose converted name exceeds 200 characters, Claude Code truncates the name to 200 characters and appends a hash of the full path" [D1].

- `E:\Programming\code-orchestrate` -> `E--Programming-code-orchestrate` (`:` and `\` each become `-`). [L] For all 11 transcripts, encoding the first record's `cwd` with that rule reproduces the folder name exactly.
- Lossy / colliding: `-`, `_`, `.`, space, `\`, `:` all map to `-`. So `E:\a-b`, `E:\a_b`, `E:\a b`, `E:\a.b` and `E:\a\b` share one folder. Different drives do not collide (`D--...` vs `E--...`, both present locally).
- Unverified: how non-ASCII letters and drive-letter case are treated. Do not try to decode; never derive a path from the folder name.
- **Decision input:** the real Workspace path comes from the `cwd` field in the transcript (as the map already settled). Two different real paths can land in one folder, so group by `cwd`, not by folder.

### 2.2 Retention

Transcripts are deleted after 30 days by default; `cleanupPeriodDays` in `settings.json` changes it [D1]. [L] `history.jsonl` references 45 session IDs but only 11 still have a transcript. Implication: Code Orchestrate's stats (Session count, total time, tokens) will **shrink** as old transcripts are swept unless the app keeps its own cache of per-Session aggregates. Recommend caching aggregates keyed by session ID.

## 3. Record types and fields [L]

Every line is one JSON object with a `type`. Counts across all inspected files:

| `type` | Count | Has `timestamp` | Use for us |
|---|---|---|---|
| `assistant` | 2668 | yes | token usage (`message.usage`), model (`message.model`) |
| `user` | 1701 | yes | activity, `cwd`; includes tool results (`toolUseResult`) and meta prompts (`isMeta`) |
| `attachment` | 2454 | yes | activity only |
| `system` | 322 | yes | `subtype`: `stop_hook_summary`, `turn_duration` (has `durationMs`), `away_summary`, `compact_boundary`, `local_command`, `informational` |
| `ai-title` | 677 | **no** | `aiTitle`: generated Session title (repeated many times, same value) |
| `last-prompt` | 720 | **no** | `lastPrompt`, `leafUuid` |
| `cost-state` | 18 | **no** | end-of-process totals (see 5.2) |
| `queue-operation` | 146 | yes | ignore |
| `mode`, `permission-mode`, `atis-latch` | 699 each | no | ignore |
| `file-history-snapshot`, `file-history-delta` | 123 / 51 | partly | checkpoints, ignore |
| `api-request`, `api-request-blob`, `api-request-shape` | 1158 / 1612 / 35 | partly | request caching internals, ignore |
| `pr-link` | 301 | yes | `prNumber`, `prUrl`, `prRepository` (nice-to-have) |
| `dev-mods` | 7 | no | ignore |

No `summary` or `custom-title` records appeared in these files (older Claude Code versions wrote `{"type":"summary", ...}`; treat as optional). Docs: picker rows show "the session name if you set one, otherwise the AI-generated session title, conversation summary, or first prompt" [D1].

Common envelope on `user` / `assistant` / `attachment` / `system`: `uuid`, `parentUuid` (null on the first record and after compaction), `isSidechain`, `timestamp` (ISO 8601 UTC with ms, e.g. `2026-10-24T04:53:06.933Z`), `sessionId`, `cwd`, `version`, `gitBranch`, `entrypoint` (`cli` or `claude-desktop`), `userType`; optional `agentId`, `slug`, `isMeta`, `isCompactSummary`.

`message.usage` (on `assistant`): `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cache_creation.{ephemeral_5m_input_tokens, ephemeral_1h_input_tokens}`, optional `output_tokens_details.thinking_tokens`, `server_tool_use.{web_search_requests, web_fetch_requests}`, `service_tier`, `speed`, `inference_geo`.

### 3.1 Two traps when summing tokens

1. **Split assistant messages.** One API response is written as several `assistant` records (one per content block) that share `message.id` and carry an identical `usage` object [L: the 11 main transcripts hold 1969 `assistant` records but only 880 unique `message.id`s; 0 ids had differing usage across their records]. Sum usage **once per `message.id`** (or per `requestId`); naive summing over-counts ~2-3x.
2. **Cache reads dominate.** `cache_read_input_tokens` is usually most of the total. Show it separately (or show "input + output" as the headline) or the number is meaningless.

### 3.2 Sidechains and subagents

- [L] All subagent records live in `<session-id>/subagents/agent-*.jsonl`, have `isSidechain: true` and an `agentId`, and their `sessionId` equals the **parent** session ID. Main transcripts contained only `isSidechain: false` records.
- For listing Sessions: read only top-level `*.jsonl` and drop any record with `isSidechain: true` defensively.
- For token stats: subagent usage is real spend. Decide whether "token usage" includes it; if yes, add the deduped usage from `subagents/*.jsonl`. [L] In one Session with 10 subagents, top-level-only tokens were ~4M versus ~36M in `cost-state.modelUsage`.

### 3.3 Other files under `~/.claude` worth knowing [L]

- `history.jsonl`: one line per prompt typed, fields `display`, `pastedContents`, `timestamp` (epoch ms), `project` (real path), `sessionId`. Contains prompt text; outlives transcripts. Useful as a fallback for "last used" per path, but it holds user content.
- `sessions/<pid>.json`: one file per **running** interactive process: `pid`, `sessionId`, `cwd`, `startedAt`, `updatedAt`, `status` (`busy` / `idle` observed), `kind` (`interactive`), `entrypoint`, `name`, `nameSource`, `version`, ... Present for live processes only (2 seen, both the currently running sessions). Strong candidate for the "Live tracking" open question. Undocumented.
- `claude agents --json` prints active sessions as a JSON array [D2]: a **documented** alternative for "is anything running here".
- `stats-cache.json`: Claude Code's own aggregate (`dailyActivity`, `dailyModelTokens`, `modelUsage`, `totalSessions`, `longestSession`, ...), global not per-project.

## 4. Session identity: Claude Code ID vs our Session

- One `.jsonl` per Claude Code session ID; file name == `sessionId` of every record in it [L: 11/11].
- **Resume reuses the ID and appends to the same file.** `--fork-session`: "When resuming, create a new session ID instead of reusing the original" [D2]; "If you resume the same session in two terminals without forking, messages from both interleave into one transcript" [D1]. `/branch` and `--fork-session` produce a new ID / new file [D1].
- `/clear` starts a new session ID (the old one stays resumable) [D1].
- Consequence for the glossary: our **Session** ("one run from start to exit") is *not* the same thing as a Claude Code session ID. One transcript file can contain several runs (original + each resume). Options for the spec: (a) treat one session ID as one Session (simple, matches the resume picker), or (b) split a file into runs. Recommend (a) for listing/resume and count, and compute "Session time" from activity (5.1), which handles multi-run files anyway.

### 4.1 Can a Session span multiple cwds?

Yes [L]: 2 of 11 transcripts have records whose `cwd` changes mid-file to a **subdirectory** of the starting cwd (the Bash tool `cd`-ing). The folder name always matched the *first* record's `cwd`. Separately, `/cd` "relocates [the session] to the new directory's project storage" [D1], i.e. the file moves folders. Rule for the app: the Session's Workspace path = `cwd` of the first record that has one (equivalently the folder it is stored under); ignore later `cwd` drift.

## 5. Deriving stats

### 5.1 Start, end, duration

- Start = earliest `timestamp`; end/last-used = latest `timestamp` in the main file. Many record types (`ai-title`, `last-prompt`, `mode`, `cost-state`, ...) have no `timestamp`; skip them. File mtime is a cheap proxy for "last used" during discovery.
- Because resumes append to the same file, `last - first` can include days of idle time. Recommend **active time** = sum of gaps between consecutive timestamps, with any gap above an idle threshold (suggest 30 min) dropped. [L] Max intra-file gap observed was 32 min; spans ranged 2 min to 4.3 h.
- `system` records with `subtype: turn_duration` carry `durationMs` per turn: an alternative "busy time" measure.

### 5.2 `cost-state` (process-exit summary) [L]

Written near the end of the file when a process exits (absent in still-running sessions and in the 2.1.156 desktop transcript; seen twice in a row, near-identical). Fields: `startTime` (epoch ms, process start), `totalDuration` (ms, wall clock of the process), `totalAPIDuration`, `totalToolDuration`, `totalCostUSD`, `totalLinesAdded`, `totalLinesRemoved`, `modelUsage.{<model>: {inputTokens, outputTokens, cacheReadInputTokens, cacheCreationInputTokens, thinkingTokens, webSearchRequests, costUSD}}`, `hasUnknownModelCost`. `startTime + totalDuration` landed within ~1-9 min of the last record timestamp. It includes subagent and title-generation (Haiku) usage. Convenient but undocumented and missing for crashed/running sessions; use as an optional enrichment, with transcript-derived figures as the baseline.

### 5.3 Title for the resume picker

Prefer, in order: user-set name (not observed in these files; set via `-n` / `/rename`), latest `ai-title.aiTitle`, then `last-prompt.lastPrompt` or the first non-meta `user` message text truncated. [L] `ai-title` repeats with one distinct value per file; one short session had none.

## 6. CLI semantics [D1, D2]

| Command | Behaviour |
|---|---|
| `claude` | New interactive session in the current directory. |
| `claude -c` / `--continue` | Most recent session **in the current directory**. Skips `claude -p` / Agent SDK sessions and sessions whose first prompt was `/loop`. Exits with an error if that most recent one is still running in the background. |
| `claude -r` / `--resume` (no arg) | Interactive picker scoped to the current worktree. |
| `claude -r <session-id>` | Resumes that ID; searches current project + its git worktrees, then every other project (since 2.1.223). Reuses the same ID / file. Error if not found: `No conversation found with session ID: <id>`. |
| `claude -r <name-or-title>` | Exact match resumes; ambiguous opens the picker pre-filtered. Generated titles work as handles; the default display name (e.g. `my-app-3f`) does not. |
| `claude -r <abs-path-to.jsonl>` | Resumes from a transcript path. |
| `--fork-session` | With `-c`/`-r`: new session ID, original untouched. |
| `--session-id <uuid>` | Start with a chosen ID. Lets the app know the transcript file name **before** launch (`<encoded>/<uuid>.jsonl`), useful for live tracking. |
| `-n <name>` | Display name, shown in `/resume` and the terminal title. |

Recommendations: for "resume a past Session" pass the session ID (exact, cross-project safe), and launch with the working directory set to the Workspace path so `--continue` and project settings resolve correctly. Resume restores conversation, model and (usually) permission mode, but not `--add-dir`, `--mcp-config`, `--settings` flags [D1].

## 7. Launching in a directory on Windows

The app should spawn processes directly with an argument array (no intermediate shell) where it can; that removes most quoting problems.

### 7.1 Windows Terminal [D4]

```
wt.exe -w new new-tab -d "E:\Programming\code-orchestrate" --title "code-orchestrate" claude.exe --resume 1234-...-uuid
```

- `-d` / `--startingDirectory` sets the tab's directory; the trailing `commandline` replaces the profile's command. `-w new` (or `-1`) forces a new window; `-w 0`/`last` reuses the most recent window. `-p "<profile>"` picks a profile. If no subcommand is given, `new-tab` is implied.
- `;` is wt's subcommand separator. A literal `;` in a path or argument must be escaped as `\;` for wt. From PowerShell the separator itself must be written `` `; `` or use `--%` [D4].
- **Trailing backslash pitfall:** `-d "C:\dir\"` - under the standard Windows argv rules a backslash before a closing quote escapes the quote, so the argument runs on. Strip trailing backslashes from paths (or double them) before quoting. (Inference from Microsoft C argv rules, not stated on [D4].)
- Paths with spaces must be quoted; when spawning with an argv array, let the runtime quote each argument.
- `--title` is overwritten by the application unless `--suppressApplicationTitle` is passed; Claude Code sets the terminal title itself (`-n` names appear there [D2]).
- `wt.exe` is an app execution alias under `%LOCALAPPDATA%\Microsoft\WindowsApps`; it may be missing or disabled, so detect it and fall back. PowerShell waits for Store apps like wt to exit when called directly [D4]; spawning from the app (or `Start-Process`) avoids that.
- `claude` here resolves to native `claude.exe`, which wt can run directly. If a user has the npm install (a `.cmd` shim), wrap it: `wt -d <dir> cmd /k claude ...` or `powershell -NoExit -Command claude ...`, since a `.cmd` cannot be the direct target of process creation. Wrapping in a shell also keeps the tab open after Claude exits, which is usually wanted.

### 7.2 PowerShell (no Windows Terminal)

```
Start-Process powershell -WorkingDirectory 'E:\Programming\my dir' -ArgumentList '-NoExit','-Command','claude --continue'
```

Single quotes avoid `$` and backtick expansion; `-WorkingDirectory` avoids embedding the path in a command string. If a path must go inside `-Command`, use `Set-Location -LiteralPath '<path>'` (paths containing `[` `]` break plain `-Path` wildcard matching) and double any `'` in the path.

### 7.3 cmd

```
start "" /D "E:\Programming\my dir" cmd /k claude --resume <uuid>
```

The empty `""` is required: `start` treats the first quoted argument as the window title. `cmd /k` keeps the window open after exit. Paths containing `&`, `^`, `%`, `(`, `)` need care inside `cmd` strings (`%` cannot be escaped on the command line); passing the directory via `/D` rather than in a `cd` string sidesteps most of this. Same trailing-backslash-before-quote pitfall as 7.1.

## 8. Implications for the spec (summary)

1. Discover Workspaces by scanning `projects/*/` top-level `*.jsonl`; take the path from the first `cwd`; group by `cwd` (folder names collide). Ignore folders with no transcripts.
2. One Claude Code session ID = one listed Session; ID == file name; resume with `--resume <id>`.
3. Tokens: dedupe by `message.id`; show cache reads separately; decide on subagent inclusion.
4. Time: activity-based with an idle cutoff, not first-to-last.
5. Cache per-Session aggregates in the app: transcripts disappear after 30 days by default.
6. Parser must be tolerant; the format is explicitly internal and unstable.
7. Live tracking candidates: `--session-id` at launch (known file), `~/.claude/sessions/<pid>.json` (undocumented, has status), `claude agents --json` (documented).
8. Launch via argv-array spawn of `wt.exe ... -d <path> claude.exe <flags>`, with cmd/PowerShell fallbacks; strip trailing backslashes; escape `;` for wt.
