//! EngineTests "Liveness retry" and "Microphone permission" (Swift EngineTests.swift :37-:233),
//! test for test, with a scripted detector factory in place of the HAL.

mod common;

use cleat_rs::config::Config;
use cleat_rs::model::{DeviceSnapshot, MicrophonePermission};
use common::engine::{Harness, Opts};
use common::*;

fn pinning_with_liveness() -> Config {
    let p = pinned_input();
    Config { input: p.input, blocked_input: p.blocked_input, liveness: p.liveness, launch_at_login: false, ..Config::default() }
}

fn run(config: &Config, snapshot: DeviceSnapshot, microphone: MicrophonePermission, starts: &[bool]) -> Harness {
    Harness::new(config, snapshot, Opts { microphone, start_results: starts.to_vec(), ..Opts::default() })
}

fn count(h: &Harness, needle: &str) -> usize {
    h.log_lines().iter().filter(|l| l.contains(needle)).count()
}

fn liveness(h: &Harness) -> String {
    h.status().liveness.get("Wireless microphone").cloned().unwrap_or_default()
}

fn running(h: &Harness) -> Vec<String> {
    let mut v: Vec<String> = h.engine.running_detector_uids().into_iter().collect();
    v.sort();
    v
}

#[test]
fn failed_detector_start_is_retried_on_later_beats_and_logged_once() {
    let snap = DeviceSnapshot { devices: vec![wireless(), brio()], default_input: Some(wireless().id), ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Granted, &[false, false, false, true]);

    // Two attempts already: the device rebind, then the reconcile that closes `start`.
    assert_eq!(h.detectors.start_count.get(), 2);
    assert!(running(&h).is_empty());

    h.engine.reconcile(false, None);
    assert_eq!(h.detectors.start_count.get(), 3);
    assert!(running(&h).is_empty());

    h.engine.reconcile(false, None);
    assert_eq!(h.detectors.start_count.get(), 4);
    assert_eq!(running(&h), vec![wireless().uid]);

    // A device that is measuring is left alone: no fifth attempt, no second line.
    h.engine.reconcile(false, None);
    assert_eq!(h.detectors.start_count.get(), 4);

    assert_eq!(count(&h, "-> unavailable"), 1);
    assert_eq!(count(&h, "-> measuring"), 1);
    assert_eq!(liveness(&h), "measuring");
}

#[test]
fn absent_device_is_not_retried_and_reports_again_on_return() {
    let snap = DeviceSnapshot { devices: vec![brio()], ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Granted, &[false]);
    assert_eq!(h.detectors.start_count.get(), 0);
    assert_eq!(liveness(&h), "absent");

    // Plugged in, and the HAL is not ready for it.
    h.set(|s| s.devices = vec![wireless(), brio()]);
    h.engine.reconcile(false, None);
    assert_eq!(h.detectors.start_count.get(), 1);
    assert_eq!(count(&h, "-> unavailable"), 1);

    // Unplugged again, then back: the second spell reports for itself.
    h.set(|s| s.devices = vec![brio()]);
    h.engine.reconcile(false, None);
    h.set(|s| s.devices = vec![wireless(), brio()]);
    h.engine.reconcile(false, None);
    assert_eq!(h.detectors.start_count.get(), 2);
    assert_eq!(count(&h, "-> unavailable"), 2);
}

#[test]
fn pending_permission_does_not_switch_to_unmeasured_liveness_device() {
    let snap = DeviceSnapshot { devices: vec![wireless(), brio()], default_input: Some(brio().id), ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Pending, &[true]);

    assert!(h.writes().is_empty());
    assert_eq!(h.detectors.start_count.get(), 0);
    assert_eq!(h.status().microphone, "not determined");
    assert_eq!(liveness(&h), "awaiting microphone permission");

    // The answer arrives: a detector measures for real, and the input stays put until a verdict.
    h.engine.update_microphone(MicrophonePermission::Granted);

    assert_eq!(h.detectors.start_count.get(), 1);
    assert_eq!(running(&h), vec![wireless().uid]);
    assert!(h.writes().is_empty());
    assert_eq!(liveness(&h), "measuring");
}

#[test]
fn denied_permission_treats_a_liveness_device_as_present() {
    let snap = DeviceSnapshot { devices: vec![wireless(), brio()], default_input: Some(brio().id), ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Pending, &[true]);
    assert!(h.writes().is_empty());

    h.engine.update_microphone(MicrophonePermission::Denied("denied".into()));

    assert_eq!(h.writes(), vec![format!("input:{}", wireless().id)]);
    assert_eq!(h.detectors.start_count.get(), 0);
    assert_eq!(h.status().microphone, "denied");
    assert_eq!(liveness(&h), "disabled (no microphone permission)");
}

#[test]
fn all_four_rules_run_before_the_permission_answer_and_detectors_follow_it() {
    let config = Config {
        output: s(&["Studio Display Speakers", "MacBook Pro Speakers"]),
        balance: Some(0.5),
        input_volume: volumes(&[("Brio 100", 75.0)]),
        ..pinning_with_liveness()
    };
    let snap = DeviceSnapshot {
        devices: vec![wireless(), brio(), air_pods(), display_speakers(), mac_speakers()],
        default_input: Some(air_pods().id),
        default_output: Some(mac_speakers().id),
        output_balance: Some(0.2),
        input_volumes: [(brio().id, 0.6f32)].into_iter().collect(),
        ..Default::default()
    };
    let mut h = run(&config, snap, MicrophonePermission::Pending, &[true]);

    let first = vec![format!("input:{}", brio().id), format!("output:{}", display_speakers().id), format!("volume:{}:0.75", brio().id)];
    assert_eq!(h.writes(), first);
    // Balance stands aside on the pass that moves the default output, and lands on the next beat.
    h.engine.reconcile(false, None);
    let mut second = first.clone();
    second.push(format!("balance:{}:0.50", display_speakers().id));
    assert_eq!(h.writes(), second);

    assert_eq!(h.detectors.start_count.get(), 0);
    assert_eq!(h.status().microphone, "not determined");
    assert_eq!(liveness(&h), "awaiting microphone permission");

    h.engine.update_microphone(MicrophonePermission::Granted);

    assert_eq!(h.detectors.start_count.get(), 1);
    assert_eq!(running(&h), vec![wireless().uid]);
    // Nothing was left to write: the locks were already held.
    assert_eq!(h.writes(), second);
    assert_eq!(h.status().microphone, "authorized");
    assert_eq!(liveness(&h), "measuring");
    assert_eq!(count(&h, "microphone: authorized"), 1);
}

#[test]
fn unchanged_permission_is_a_no_op() {
    let snap = DeviceSnapshot { devices: vec![wireless()], default_input: Some(wireless().id), ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Granted, &[true]);
    let before = h.log_lines().len();

    h.engine.update_microphone(MicrophonePermission::Granted);

    assert_eq!(h.log_lines().len(), before);
    assert_eq!(h.detectors.start_count.get(), 1);
    assert_eq!(h.detectors.stop_count.get(), 0);
}

/// Not in Swift's suite: a flip reaches status and the log, and a flip for a device whose
/// detector is gone is dropped.
#[test]
fn flips_reach_status_and_stale_flips_are_dropped() {
    use cleat_rs::engine::Event;
    let snap = DeviceSnapshot { devices: vec![wireless(), brio()], default_input: Some(wireless().id), ..Default::default() };
    let mut h = run(&pinning_with_liveness(), snap, MicrophonePermission::Granted, &[true]);
    let flip = |live| Event::LivenessFlip { uid: wireless().uid, name: wireless().name, live };

    h.engine.handle(flip(false));
    assert_eq!(liveness(&h), "silent");
    assert_eq!(count(&h, "liveness: Wireless microphone -> silent"), 1);
    // Silent receiver: the input moves to the next device on the list.
    assert_eq!(h.writes(), vec![format!("input:{}", brio().id)]);

    h.engine.handle(flip(true));
    assert_eq!(liveness(&h), "live");

    h.set(|s| s.devices = vec![brio()]);
    h.engine.reconcile(false, None);
    assert_eq!(count(&h, "liveness: Wireless microphone -> stopped"), 1);
    let lines = h.log_lines().len();
    h.engine.handle(flip(false));
    assert_eq!(h.log_lines().len(), lines);
}
