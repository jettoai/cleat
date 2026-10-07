//! The deadline is on this side of the pipe (Swift `BluetoothHeadsets.swift:182-197`, the three
//! shapes measured there): a child that ignores SIGTERM, and one whose grandchild holds the pipe
//! after a clean exit with half a document, both cost the caller the timeout and no more.

use std::time::{Duration, Instant};

use cleat_rs::reclaim::child::run_with_deadline;

const LIMIT: Duration = Duration::from_secs(1);
const CEILING: Duration = Duration::from_millis(1300);

fn timed(script: &str) -> (Option<Vec<u8>>, Duration) {
    let start = Instant::now();
    let out = run_with_deadline("/bin/sh", &["-c", script], LIMIT);
    (out, start.elapsed())
}

#[test]
fn child_ignoring_sigterm_costs_only_the_timeout() {
    let (out, took) = timed(r#"trap "" TERM; sleep 12"#);
    assert!(out.is_none());
    assert!(took <= CEILING, "took {took:?}");
}

#[test]
fn grandchild_holding_the_pipe_after_a_clean_half_answer_costs_only_the_timeout() {
    let (out, took) = timed(r#"(sleep 9 &) ; printf "{\"x\":"; exit 0"#);
    assert!(out.is_none());
    assert!(took <= CEILING, "took {took:?}");
}

#[test]
fn clean_answer_is_returned() {
    assert_eq!(timed("printf ok").0, Some(b"ok".to_vec()));
}

#[test]
fn bad_exit_is_no_answer() {
    assert_eq!(timed("printf ok; exit 3").0, None);
}
