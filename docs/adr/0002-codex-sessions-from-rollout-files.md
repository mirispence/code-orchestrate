# Codex Sessions are read from rollout files, not Codex's SQLite state

The Codex adapter reads each Session from its compressed rollout file (`~/.codex/sessions/**/rollout-*.jsonl.zst` and `archived_sessions/`), decoded in Rust with the `zstd` crate, rather than from Codex's own `state_N.sqlite` database. One file per Session fits the existing scan model (source key = path + mtime + size, re-parse only what changed), and the rollout's `token_count` events carry the full token breakdown, where the database holds only a total.

## Considered Options

- **Codex's `state_N.sqlite` `threads` table**: less parsing (cwd, title, timestamps, archived flag ready-made), rejected because it reports only total tokens, its file name carries a schema version (`state_5`) that signals churn, and one database for every Session breaks the per-file source key.

## Consequences

- The app takes a native `zstd` dependency (built from source by the crate; no system install needed).
- Session titles are not in the rollout; they come from Codex's `session_index.jsonl` (`thread_name`).
- If Codex changes its rollout format, only `agents::codex` changes; the trait and store are unaffected.
