//! Swift `EventLog.swift`: only actions that changed something are written. A converged daemon
//! in enforce mode writes the started line and nothing else; trace mode adds the `pass:` lines.

mod common;

use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};
use common::*;

fn converged() -> DeviceSnapshot {
    DeviceSnapshot {
        devices: vec![wireless(), brio(), display_speakers()],
        default_input: Some(wireless().id),
        default_output: Some(display_speakers().id),
        output_balance: Some(0.5),
        input_volumes: [(wireless().id, 1.0f32), (brio().id, 1.0)].into_iter().collect(),
        ..Default::default()
    }
}

const CONFIG: &str = r#"{
  "input": ["Wireless microphone", "Brio 100"],
  "output": ["Studio Display Speakers"],
  "balance": 0.5,
  "inputVolume": {"*": 100}
}"#;

#[test]
fn a_converged_daemon_writes_only_the_started_line() {
    let mut h = Harness::with_json(CONFIG, converged(), Opts::default());
    for _ in 0..10 {
        h.engine.reconcile(false, None);
    }
    let lines = h.log_lines();
    assert!(h.writes().is_empty());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].ends_with(" engine started (config: ok, microphone: authorized)"), "{lines:?}");
}

#[test]
fn trace_mode_writes_the_pass_lines() {
    let mut h = Harness::with_json(CONFIG, converged(), Opts { trace: true, ..Opts::default() });
    for _ in 0..10 {
        h.engine.reconcile(false, None);
    }
    assert_eq!(h.log_lines().iter().filter(|l| l.contains(" pass: ")).count(), 11);
}

/// The log line prefix is Swift's `yyyy-MM-dd HH:mm:ss`, to the second.
#[test]
fn log_lines_carry_a_seconds_timestamp() {
    let h = Harness::with_json(CONFIG, converged(), Opts::default());
    let line = &h.log_lines()[0];
    let (stamp, _) = line.split_at(19);
    let shape: String = stamp.chars().map(|c| if c.is_ascii_digit() { 'd' } else { c }).collect();
    assert_eq!(shape, "dddd-dd-dd dd:dd:dd");
    assert_eq!(&line[19..20], " ");
}
