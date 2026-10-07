//! Proof that observe mode decides but never reaches a writer, the launch agent or the error
//! reporter, with an enforce-mode control showing the same fixture does reach each of them.

mod common;

use cleat_rs::engine::Mode;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{release_identity, Harness, Opts};
use common::*;

/// Release identity, `launchAtLogin` on and `errorReports` on, so every side effect is wanted.
const FOUR_RULES: &str = r#"{
  "input": ["Wireless microphone", "Brio 100"],
  "output": ["Studio Display Speakers"],
  "blockedOutput": ["MacBook Pro Speakers"],
  "inputVolume": {"*": 100},
  "errorReports": true
}"#;

fn four_rules_snapshot() -> DeviceSnapshot {
    DeviceSnapshot {
        devices: vec![wireless(), brio(), display_speakers(), mac_speakers()],
        default_input: Some(brio().id),
        default_output: Some(mac_speakers().id),
        input_volumes: [(wireless().id, 0.5f32)].into_iter().collect(),
        ..Default::default()
    }
}

fn run(mode: Mode, config: &str, snapshot: DeviceSnapshot) -> Harness {
    Harness::with_json(config, snapshot, Opts { mode, identity: release_identity(), ..Opts::default() })
}

fn log(h: &Harness) -> String {
    h.log_lines().join("\n")
}

#[test]
fn observe_decides_but_never_writes() {
    let h = run(Mode::Observe, FOUR_RULES, four_rules_snapshot());
    assert_eq!(h.writes().len(), 0, "observe mode reached a writer: {:?}", h.writes());
    let log = log(&h);
    assert!(log.contains("pinInput: Brio 100 -> Wireless microphone (higher priority present) [observe: not applied]"));
    assert!(log.contains("pinOutput: MacBook Pro Speakers -> Studio Display Speakers (blocked) [observe: not applied]"));
    assert!(log.contains("inputVolume: Wireless microphone 50% -> 100% [observe: not applied]"));
    assert!(log.contains("actions=3"));
    assert!(log.contains("engine started (config: ok, microphone: authorized, mode: observe)"));
}

#[test]
fn observe_never_touches_the_launch_agent_or_the_reporter() {
    let h = run(Mode::Observe, FOUR_RULES, four_rules_snapshot());
    assert_eq!(h.agent.registers.get() + h.agent.unregisters.get() + h.agent.status_reads.get(), 0);
    assert_eq!(h.login_item.status_reads.get(), 0);
    assert!(h.reports.borrow().is_empty());
}

#[test]
fn observe_never_writes_balance() {
    let snapshot = DeviceSnapshot {
        devices: vec![display_speakers()],
        default_output: Some(display_speakers().id),
        output_balance: Some(0.3),
        ..Default::default()
    };
    let h = run(Mode::Observe, r#"{"balance": 0.5}"#, snapshot);
    assert_eq!(h.writes().len(), 0, "observe mode reached a writer: {:?}", h.writes());
    assert!(log(&h).contains("balance: Studio Display Speakers 0.30 -> 0.50 [observe: not applied]"));
}

#[test]
fn enforce_mode_reaches_the_writer_the_agent_and_the_reporter() {
    let h = run(Mode::Enforce, FOUR_RULES, four_rules_snapshot());
    assert_eq!(h.writes(), vec!["input:10", "output:60", "volume:10:1.00"]);
    let log = log(&h);
    assert!(!log.contains("[observe: not applied]"));
    assert!(log.contains("pinInput: Brio 100 -> Wireless microphone (higher priority present)"));
    assert_eq!(h.agent.registers.get(), 1);
    assert_eq!(*h.reports.borrow(), vec![true]);
    assert!(log.contains("errorReports: on"));
}

#[test]
fn nothing_to_do_still_writes_a_pass_line_in_observe_mode() {
    let h = run(Mode::Observe, "{}", four_rules_snapshot());
    assert!(h.writes().is_empty());
    let log = log(&h);
    assert!(log.contains("pass: trigger=startup"));
    assert!(log.contains("actions=0 (nothing to do)"));
}

/// P2 column: observe mode opens no silence detector even with the microphone granted.
#[test]
fn observe_never_makes_a_detector_and_enforce_does() {
    let config = r#"{"input": ["Wireless microphone", "Brio 100"], "liveness": {"Wireless microphone": {"zeroSeconds": 3}}}"#;
    let snapshot = || DeviceSnapshot { devices: vec![wireless(), brio()], default_input: Some(wireless().id), ..Default::default() };
    let observe = run(Mode::Observe, config, snapshot());
    assert_eq!(observe.detectors.made.get(), 0);
    let enforce = run(Mode::Enforce, config, snapshot());
    assert!(enforce.detectors.made.get() >= 1);
}
