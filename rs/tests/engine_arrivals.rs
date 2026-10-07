//! EngineTests "Headphone arrivals" (Swift EngineTests.swift :240-:536), test for test.

mod common;

use cleat_rs::config::Config;
use cleat_rs::engine::{consumes_arrivals, BALANCE_SETTLE_MS, RETRY_MS, SETTLE_MS};
use cleat_rs::model::{AudioDevice, DeviceSnapshot, TRANSPORT_USB};
use common::engine::{Harness, Opts};
use common::*;

/// Albert's output side: speakers as fallback, the microphone's speaker end blocked, takeover on.
fn headphones_config() -> Config {
    Config {
        output: s(&["Mac Studio的揚聲器"]),
        blocked_output: s(&["Maono AI Microphone"]),
        headphones_take_over: true,
        launch_at_login: false,
        ..Config::default()
    }
}

fn snapshot(devices: Vec<AudioDevice>, output: u32) -> DeviceSnapshot {
    DeviceSnapshot { devices, default_output: Some(output), ..Default::default() }
}

fn count(h: &Harness, needle: &str) -> usize {
    h.log_lines().iter().filter(|l| l.contains(needle)).count()
}

#[test]
fn headphones_take_over_on_arrival_only_and_leave_a_hand_picked_output_alone() {
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(
        &headphones_config(),
        snapshot(vec![speakers.clone(), air_pods()], speakers.id),
        Opts::default(),
    );
    assert!(h.writes().is_empty());

    h.set(|s| s.devices = vec![speakers.clone()]);
    h.engine.reconcile(true, None);
    assert!(h.writes().is_empty());

    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
    assert_eq!(count(&h, "pinOutput: Mac Studio的揚聲器 -> AirPods Max (headphones connected)"), 1);

    h.engine.reconcile(true, None);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);

    h.set(|s| s.default_output = Some(speakers.id));
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);

    assert_eq!(
        h.rule("headphones"),
        "on (bluetooth output takes over when it connects; blocked from taking over: Maono AI Microphone)"
    );
    assert_eq!(h.rule("outputPin"), "on (Mac Studio的揚聲器), blocked: Maono AI Microphone");
}

#[test]
fn output_list_takes_over_once_the_headphones_are_gone() {
    let maono_speakers =
        AudioDevice::with_transport(51, "Maono\u{00A0}AI Microphone", "Maono-Output-UID", false, true, TRANSPORT_USB);
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(
        &headphones_config(),
        snapshot(vec![speakers.clone(), maono_speakers.clone(), air_pods()], air_pods().id),
        Opts::default(),
    );
    assert!(h.writes().is_empty());

    h.set(|s| {
        s.devices = vec![speakers.clone(), maono_speakers.clone()];
        s.default_output = Some(maono_speakers.id);
    });
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", speakers.id)]);
    assert_eq!(count(&h, "pinOutput: Maono\u{00A0}AI Microphone -> Mac Studio的揚聲器 (blocked)"), 1);
}

#[test]
fn only_the_settle_beat_and_later_consume_arrivals() {
    assert!(!consumes_arrivals(0));
    assert!(!consumes_arrivals(BALANCE_SETTLE_MS));
    assert!(consumes_arrivals(SETTLE_MS));
    for beat in RETRY_MS {
        assert!(consumes_arrivals(beat), "beat {beat}");
    }
}

#[test]
fn early_beat_that_did_not_stick_leaves_the_arrival_for_the_settle_beat() {
    let speakers = mac_studio_speakers();
    let airpods = format!("output:{}", air_pods().id);
    let mut h = Harness::new(&headphones_config(), snapshot(vec![speakers.clone()], speakers.id), Opts::default());
    assert!(h.writes().is_empty());

    h.audio.output_writes_stick.set(false);
    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(false, None);
    assert_eq!(h.writes(), vec![airpods.clone()]);
    assert_eq!(h.audio.snapshot.borrow().default_output, Some(speakers.id));

    h.audio.output_writes_stick.set(true);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![airpods.clone(), airpods.clone()]);
    assert_eq!(h.audio.snapshot.borrow().default_output, Some(air_pods().id));

    h.set(|s| s.default_output = Some(speakers.id));
    h.engine.reconcile(true, None);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![airpods.clone(), airpods]);
}

#[test]
fn early_beat_that_stuck_is_not_repeated_on_the_settle_beat() {
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(&headphones_config(), snapshot(vec![speakers.clone()], speakers.id), Opts::default());

    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(false, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
    assert_eq!(h.audio.snapshot.borrow().default_output, Some(air_pods().id));

    h.engine.reconcile(true, None);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
    assert_eq!(count(&h, "(headphones connected)"), 1);
}

#[test]
fn listed_headset_is_not_put_back_on_the_speakers_by_the_next_beat() {
    let config = Config { output: s(&["Mac Studio的揚聲器", "AirPods Max"]), ..headphones_config() };
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(&config, snapshot(vec![speakers.clone()], speakers.id), Opts::default());

    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);

    h.engine.reconcile(true, None);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
}

#[test]
fn listed_headset_is_not_taken_back_after_the_user_picks_the_speakers() {
    let config = Config { output: s(&["AirPods Max", "Mac Studio的揚聲器"]), ..headphones_config() };
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(&config, snapshot(vec![speakers.clone(), air_pods()], air_pods().id), Opts::default());
    assert!(h.writes().is_empty());

    h.set(|s| s.default_output = Some(speakers.id));
    h.engine.reconcile(true, None);
    h.engine.reconcile(true, None);
    assert!(h.writes().is_empty());
}

#[test]
fn unscheduled_reconcile_does_not_spend_the_arrival() {
    let speakers = mac_studio_speakers();
    let airpods = format!("output:{}", air_pods().id);
    let mut h = Harness::new(&headphones_config(), snapshot(vec![speakers.clone()], speakers.id), Opts::default());

    h.audio.output_writes_stick.set(false);
    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(false, None); // the default: a pass nobody scheduled
    assert_eq!(h.writes(), vec![airpods.clone()]);

    h.audio.output_writes_stick.set(true);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![airpods.clone(), airpods]);
    assert_eq!(h.audio.snapshot.borrow().default_output, Some(air_pods().id));
}

#[test]
fn first_arrival_after_start_is_still_seen() {
    let speakers = mac_studio_speakers();
    let mut h = Harness::new(&headphones_config(), snapshot(vec![speakers.clone()], speakers.id), Opts::default());

    h.set(|s| s.devices = vec![speakers.clone(), air_pods()]);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
}
