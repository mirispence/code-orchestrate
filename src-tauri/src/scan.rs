//! Session scan orchestration (spec §5.1): walk an Agent's transcripts,
//! re-parse only files whose source key changed, and save the results.
//!
//! [`run`] is one scan. The store lock is held only to read source keys and
//! to write results, never while reading files, so commands stay responsive.
//! [`Scanner`] runs scans on a background thread and coalesces triggers.

use std::sync::{Arc, Mutex};

use crate::agents::AgentAdapter;
use crate::store::{ScannedSource, Store, StoreError};

/// What one scan did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanReport {
    /// Transcript files found.
    pub sources: usize,
    /// Files parsed because their source key changed (or was new).
    pub parsed: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("The store lock was poisoned")]
    Poisoned,
}

/// Scan `agent`'s transcript store into `store`. Session records are never
/// deleted, even when their transcript is gone (spec §4.3).
pub fn run(store: &Mutex<Store>, agent: &dyn AgentAdapter) -> Result<ScanReport, ScanError> {
    scan(store, agent, false)
}

/// Settings › Rescan transcripts: re-parse every file still on disk,
/// whatever its source key. Like [`run`], it never deletes a record.
pub fn run_full(store: &Mutex<Store>, agent: &dyn AgentAdapter) -> Result<ScanReport, ScanError> {
    scan(store, agent, true)
}

fn scan(store: &Mutex<Store>, agent: &dyn AgentAdapter, full: bool) -> Result<ScanReport, ScanError> {
    let known = store.lock().map_err(|_| ScanError::Poisoned)?.source_keys(agent.id())?;
    let sources = agent.list_sources();
    let changed: Vec<&_> = sources
        .iter()
        .filter(|key| full || known.get(key.path.to_string_lossy().as_ref()) != Some(&(key.mtime, key.size)))
        .collect();
    // A file that cannot be read now keeps no source key, so the next scan retries it.
    let parsed: Vec<ScannedSource> = changed
        .iter()
        .filter_map(|key| {
            let session = agent.parse_source(&key.path).ok()?;
            Some(ScannedSource { key: (*key).clone(), session })
        })
        .collect();
    if !parsed.is_empty() {
        store.lock().map_err(|_| ScanError::Poisoned)?.record_scan(agent.id(), &parsed)?;
    }
    Ok(ScanReport { sources: sources.len(), parsed: changed.len() })
}

/// Runs scans on a background thread, one at a time. A trigger during a scan
/// queues at most one follow-up, so a burst of triggers costs two scans at
/// most. The job gets `true` for a full rescan ([`Scanner::trigger_full`]);
/// a full request during a scan makes the follow-up full. `on_state(true)` fires when scanning starts and `on_state(false)`
/// when it stops (after any follow-up).
#[derive(Clone)]
pub struct Scanner {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<State>,
    job: Box<dyn Fn(bool) + Send + Sync>,
    on_state: Box<dyn Fn(bool) + Send + Sync>,
}

#[derive(Default)]
struct State {
    running: bool,
    pending: bool,
    /// The pending (or about-to-start) scan is a full rescan.
    full: bool,
}

impl Scanner {
    pub fn new(job: impl Fn(bool) + Send + Sync + 'static, on_state: impl Fn(bool) + Send + Sync + 'static) -> Self {
        Scanner {
            inner: Arc::new(Inner { state: Mutex::default(), job: Box::new(job), on_state: Box::new(on_state) }),
        }
    }

    /// Start a scan now, or queue one follow-up if a scan is running.
    pub fn trigger(&self) {
        self.start(false);
    }

    /// Like [`Scanner::trigger`], but the scan re-parses every file (Rescan).
    pub fn trigger_full(&self) {
        self.start(true);
    }

    fn start(&self, full: bool) {
        let mut state = lock(&self.inner.state);
        state.full |= full;
        if state.running {
            state.pending = true;
            return;
        }
        state.running = true;
        // Reported under the lock so start/stop notices can never reorder.
        (self.inner.on_state)(true);
        drop(state);
        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            loop {
                let full = std::mem::take(&mut lock(&inner.state).full);
                // A panicking job must not leave the Scanner stuck "running".
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (inner.job)(full)));
                let mut state = lock(&inner.state);
                if state.pending {
                    state.pending = false;
                    continue;
                }
                state.running = false;
                (inner.on_state)(false);
                break;
            }
        });
    }

    pub fn is_running(&self) -> bool {
        lock(&self.inner.state).running
    }
}

/// The state stays consistent even if a job panicked, so recover the guard.
fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}
