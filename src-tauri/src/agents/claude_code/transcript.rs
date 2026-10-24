//! Claude Code's JSONL transcript store (spec §5.2). The format is internal to
//! Claude Code and changes between versions, so everything here is defensive:
//! unknown record types and fields are ignored, missing ones are tolerated,
//! and a line that does not parse (such as a half-written last line) is
//! skipped.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::agents::{SessionSummary, SourceKey};
use crate::paths::normalize;

/// Every `<projects>/<folder>/<session-id>.jsonl`. Subfolders (subagents,
/// tool results, memory) are not Sessions and are not listed.
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
            sources.push(SourceKey { path, mtime: mtime_nanos(&meta), size: meta.len() });
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
    Ok(summarize(session_id, path, &String::from_utf8_lossy(&bytes)))
}

fn summarize(session_id: String, path: &Path, text: &str) -> Option<SessionSummary> {

    let mut first_cwd = None;
    let mut started_at: Option<i64> = None;
    let mut last_activity_at: Option<i64> = None;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else { continue };
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if first_cwd.is_none() {
            first_cwd = record.get("cwd").and_then(Value::as_str).and_then(|c| normalize(c).ok());
        }
        if let Some(t) = record.get("timestamp").and_then(Value::as_str).and_then(parse_timestamp) {
            started_at = Some(started_at.map_or(t, |s| s.min(t)));
            last_activity_at = Some(last_activity_at.map_or(t, |l| l.max(t)));
        }
    }
    Some(SessionSummary {
        session_id,
        first_cwd: first_cwd?,
        started_at,
        last_activity_at,
        source_path: PathBuf::from(path),
    })
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
