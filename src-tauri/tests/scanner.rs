//! The background scan coordinator (spec §5.1): scans run off the calling
//! thread, report when they start and finish, and triggers that arrive during
//! a scan coalesce into at most one follow-up scan.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use code_orchestrate_lib::scan::Scanner;

const WAIT: Duration = Duration::from_secs(5);

/// A scan job that announces each start and then waits to be released.
struct Gate {
    started: Receiver<()>,
    release: Sender<()>,
    states: Receiver<bool>,
    scanner: Scanner,
}

fn gated() -> Gate {
    let (started_tx, started) = channel();
    let (release, release_rx) = channel::<()>();
    let (states_tx, states) = channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    let started_tx = Mutex::new(started_tx);
    let states_tx = Mutex::new(states_tx);
    let scanner = Scanner::new(
        move || {
            started_tx.lock().unwrap().send(()).unwrap();
            release_rx.lock().unwrap().recv_timeout(WAIT).unwrap();
        },
        move |running| states_tx.lock().unwrap().send(running).unwrap(),
    );
    Gate { started, release, states, scanner }
}

#[test]
fn a_trigger_runs_one_scan_in_the_background() {
    let g = gated();

    g.scanner.trigger();

    // The scan is running on another thread while this one carries on.
    g.started.recv_timeout(WAIT).unwrap();
    assert!(g.scanner.is_running());
    assert_eq!(g.states.recv_timeout(WAIT), Ok(true));
    g.release.send(()).unwrap();
    assert_eq!(g.states.recv_timeout(WAIT), Ok(false));
    assert!(!g.scanner.is_running());
    assert!(g.started.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn triggers_during_a_scan_coalesce_into_one_follow_up() {
    let g = gated();
    g.scanner.trigger();
    g.started.recv_timeout(WAIT).unwrap();

    g.scanner.trigger();
    g.scanner.trigger();
    g.scanner.trigger();
    g.release.send(()).unwrap();

    // Exactly one more scan, with no "finished" in between.
    g.started.recv_timeout(WAIT).unwrap();
    g.release.send(()).unwrap();
    assert_eq!(g.states.recv_timeout(WAIT), Ok(true));
    assert_eq!(g.states.recv_timeout(WAIT), Ok(false));
    assert!(g.started.recv_timeout(Duration::from_millis(200)).is_err());
    assert!(g.states.try_recv().is_err());
}

/// A clock the test moves by hand.
#[derive(Clone)]
struct FakeClock {
    base: std::time::Instant,
    offset: Arc<Mutex<Duration>>,
}

impl FakeClock {
    fn new() -> Self {
        FakeClock { base: std::time::Instant::now(), offset: Arc::default() }
    }
    fn advance(&self, by: Duration) {
        *self.offset.lock().unwrap() += by;
    }
    fn now(&self) -> std::time::Instant {
        self.base + *self.offset.lock().unwrap()
    }
}

/// A scanner whose scans finish at once, counting how many ran.
fn counting(clock: &FakeClock) -> (Scanner, Receiver<()>, Receiver<bool>) {
    let (started_tx, started) = channel();
    let (states_tx, states) = channel();
    let started_tx = Mutex::new(started_tx);
    let states_tx = Mutex::new(states_tx);
    let clock = clock.clone();
    let scanner = Scanner::with_clock(
        move || started_tx.lock().unwrap().send(()).unwrap(),
        move |running| states_tx.lock().unwrap().send(running).unwrap(),
        move || clock.now(),
    );
    (scanner, started, states)
}

/// Wait until the scanner reports a scan finished.
fn finished(states: &Receiver<bool>) {
    assert_eq!(states.recv_timeout(WAIT), Ok(true));
    assert_eq!(states.recv_timeout(WAIT), Ok(false));
}

fn no_scan(started: &Receiver<()>) {
    assert!(started.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn focus_scans_run_at_most_once_per_ten_seconds() {
    let clock = FakeClock::new();
    let (scanner, started, states) = counting(&clock);

    scanner.focus();
    started.recv_timeout(WAIT).unwrap();
    finished(&states);

    clock.advance(Duration::from_secs(9));
    scanner.focus();
    no_scan(&started);

    clock.advance(Duration::from_secs(1));
    scanner.focus();
    started.recv_timeout(WAIT).unwrap();
    finished(&states);
}

#[test]
fn a_startup_or_refresh_scan_counts_toward_the_focus_throttle() {
    let clock = FakeClock::new();
    let (scanner, started, states) = counting(&clock);

    scanner.trigger();
    started.recv_timeout(WAIT).unwrap();
    finished(&states);

    // The window gaining focus right after startup does not scan again.
    clock.advance(Duration::from_secs(2));
    scanner.focus();
    no_scan(&started);
}

#[test]
fn refresh_is_never_throttled() {
    let clock = FakeClock::new();
    let (scanner, started, states) = counting(&clock);

    scanner.focus();
    started.recv_timeout(WAIT).unwrap();
    finished(&states);

    scanner.trigger();
    started.recv_timeout(WAIT).unwrap();
    finished(&states);
}
