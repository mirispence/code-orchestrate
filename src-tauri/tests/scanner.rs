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
