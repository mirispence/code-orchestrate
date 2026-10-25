//! Claude Code's JSONL transcript store (spec §5.2). The format is internal to
//! Claude Code and changes between versions, so everything here is defensive:
//! unknown record types and fields are ignored, missing ones are tolerated,
//! and a line that does not parse (such as a half-written last line) is
//! skipped.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::agents::{SessionSummary, SourceKey, TokenUsage};
use crate::paths::normalize;

/// Every `<projects>/<folder>/<session-id>.jsonl`. Subfolders (subagents,
/// tool results, memory) are not Sessions and are not listed, but a Session's
/// subagent files count toward its key.
pub fn list_sources(projects: &Path) -> Vec<SourceKey> {
    let Ok(folders) = std::fs::read_dir(projects) else { return Vec::new() };
    let mut sources = Vec::new();
    for folder in folders.flatten() {
        let Ok(files) = std::fs::read_dir(folder.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if !is_jsonl(&path) {
                continue;
            }
            let Ok(meta) = file.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            // A Session's key covers its subagent files too, so a change to
            // one re-parses the parent (spec §5.2): newest mtime, total size.
            let mut key = SourceKey { mtime: mtime_nanos(&meta), size: meta.len(), path };
            for sub in subagent_files(&key.path) {
                let Ok(sub_meta) = std::fs::metadata(&sub) else { continue };
                key.mtime = key.mtime.max(mtime_nanos(&sub_meta));
                key.size += sub_meta.len();
            }
            sources.push(key);
        }
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources
}

/// One file = one Session; the file name is the session ID. The working
/// directory is the first record's `cwd`, never the folder name (the folder
/// encoding is lossy).
pub fn parse(path: &Path) -> std::io::Result<Option<SessionSummary>> {
    let Some(session_id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
        return Ok(None);
    };
    let bytes = std::fs::read(path)?;
    let mut usage = Usage::default();
    // Subagent spend is real spend: fold it into the parent (spec §5.2).
    for file in subagent_files(path) {
        let bytes = std::fs::read(&file)?;
        for record in records(&String::from_utf8_lossy(&bytes)) {
            usage.add(&record);
        }
    }
    Ok(summarize(session_id, path, &String::from_utf8_lossy(&bytes), usage))
}

/// `<folder>/<session-id>/subagents/*.jsonl`, sorted; none when the folder is missing.
fn subagent_files(main: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(main.with_extension("").join("subagents")) else { return Vec::new() };
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| is_jsonl(p) && p.is_file()).collect();
    files.sort();
    files
}

/// The lines that parse as JSON; others (a half-written last line) are skipped.
fn records(text: &str) -> impl Iterator<Item = Value> + '_ {
    text.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

/// A gap between consecutive records longer than this is idle time (spec §5.2).
const IDLE_CUTOFF_MS: i64 = 30 * 60 * 1_000;

fn summarize(session_id: String, path: &Path, text: &str, mut usage: Usage) -> Option<SessionSummary> {
    let mut first_cwd = None;
    let mut times: Vec<i64> = Vec::new();
    let mut titles = Titles::default();
    for record in records(text) {
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        usage.add(&record);
        titles.add(&record);
        if first_cwd.is_none() {
            first_cwd = record.get("cwd").and_then(Value::as_str).and_then(|c| normalize(c).ok());
        }
        if let Some(t) = record.get("timestamp").and_then(Value::as_str).and_then(parse_timestamp) {
            times.push(t);
        }
    }
    times.sort_unstable();
    Some(SessionSummary {
        session_id,
        first_cwd: first_cwd?,
        title: titles.best(),
        started_at: times.first().copied(),
        last_activity_at: times.last().copied(),
        active_time_ms: Some(active_time(&times)),
        tokens: usage.total,
        source_path: PathBuf::from(path),
    })
}

/// A prompt used as a title is cut to this many characters, ellipsis included.
const PROMPT_TITLE_CHARS: usize = 80;

/// Title candidates, in precedence order (spec §5.2).
#[derive(Default)]
struct Titles {
    /// The latest user-set name (`-n` / `/rename`).
    custom: Option<String>,
    ai: Option<String>,
    last_prompt: Option<String>,
    first_prompt: Option<String>,
}

impl Titles {
    fn add(&mut self, record: &Value) {
        let text = |field: &str| {
            record.get(field).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned)
        };
        match record.get("type").and_then(Value::as_str) {
            Some("custom-title") => self.custom = text("customTitle").or(self.custom.take()),
            Some("ai-title") => self.ai = text("aiTitle").or(self.ai.take()),
            Some("last-prompt") => self.last_prompt = text("lastPrompt").or(self.last_prompt.take()),
            Some("user") if self.first_prompt.is_none() => self.first_prompt = prompt_text(record),
            _ => {}
        }
    }

    fn best(self) -> Option<String> {
        self.custom
            .or(self.ai)
            .or_else(|| self.last_prompt.map(|p| one_line(&p)))
            .or_else(|| self.first_prompt.map(|p| one_line(&p)))
    }
}

/// The text a person typed in a `user` record; `None` for meta records and
/// tool results.
fn prompt_text(record: &Value) -> Option<String> {
    if record.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = record.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.as_str(),
        Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")) {
                return None;
            }
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .find_map(|b| b.get("text").and_then(Value::as_str))?
        }
        _ => return None,
    };
    Some(text.trim()).filter(|t| !t.is_empty()).map(str::to_owned)
}

/// Whitespace collapsed to single spaces, cut to [`PROMPT_TITLE_CHARS`].
fn one_line(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= PROMPT_TITLE_CHARS {
        return collapsed;
    }
    let cut: String = collapsed.chars().take(PROMPT_TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Token usage summed once per response. One API response is written as
/// several `assistant` records sharing a `message.id` and repeating its usage.
struct Usage {
    seen: HashSet<String>,
    total: TokenUsage,
}

impl Default for Usage {
    /// Claude Code reports all four fields, so a Session with no responses has zeros.
    fn default() -> Self {
        Usage { seen: HashSet::new(), total: TokenUsage::ZERO }
    }
}

impl Usage {
    fn add(&mut self, record: &Value) {
        if record.get("type").and_then(Value::as_str) != Some("assistant") {
            return;
        }
        let Some(message) = record.get("message") else { return };
        let Some(usage) = message.get("usage").filter(|u| u.is_object()) else { return };
        // A response without an id cannot be deduplicated; count it once as is.
        if let Some(id) = message.get("id").and_then(Value::as_str) {
            if !self.seen.insert(id.to_owned()) {
                return;
            }
        }
        let field = |name: &str| Some(usage.get(name).and_then(Value::as_i64).unwrap_or(0));
        self.total = self.total.plus(TokenUsage {
            input: field("input_tokens"),
            output: field("output_tokens"),
            cache_write: field("cache_creation_input_tokens"),
            cache_read: field("cache_read_input_tokens"),
        });
    }
}

/// The sum of the gaps between consecutive sorted times, dropping idle gaps.
fn active_time(sorted: &[i64]) -> i64 {
    sorted.windows(2).map(|w| w[1] - w[0]).filter(|gap| *gap <= IDLE_CUTOFF_MS).sum()
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
}

fn mtime_nanos(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// An RFC 3339 timestamp (`2026-10-24T04:53:06.933Z`, or with a `+hh:mm`
/// offset) as Unix epoch milliseconds.
fn parse_timestamp(raw: &str) -> Option<i64> {
    let b = raw.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let s = raw.get(from..from + len)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut i = 19;
    let mut millis = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return None;
        }
        // Keep milliseconds; pad or truncate the fraction to three digits.
        let frac = &raw[start..i];
        let padded = format!("{:0<3}", &frac[..frac.len().min(3)]);
        millis = padded.parse::<i64>().ok()?;
    }
    // Compared as bytes: a malformed suffix may hold multi-byte characters.
    let offset_minutes = match &b[i..] {
        b"Z" | b"z" => 0,
        [sign @ (b'+' | b'-'), _, _, b':', _, _] => {
            let sign = if *sign == b'-' { -1 } else { 1 };
            sign * (num(i + 1, 2)? * 60 + num(i + 4, 2)?)
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_minutes * 60;
    Some(seconds * 1_000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
