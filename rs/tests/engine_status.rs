//! EngineTests status lines (Swift EngineTests.swift :539-:676), strings word for word.

mod common;

use cleat_rs::config::Config;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};
use common::*;

/// Albert's input pinning plus the one liveness entry, login item left out.
fn pinning_with_liveness() -> Config {
    Config { launch_at_login: false, ..pinned_input() }
}

fn input_snapshot(devices: Vec<cleat_rs::model::AudioDevice>, volumes: &[(u32, f32)]) -> DeviceSnapshot {
    DeviceSnapshot {
        devices,
        default_input: Some(brio().id),
        input_volumes: volumes.iter().copied().collect(),
        ..Default::default()
    }
}

#[test]
fn blocked_list_without_a_priority_list_is_on_for_eviction() {
    let config = Config {
        output: vec![],
        blocked_output: s(&["Maono AI Microphone"]),
        headphones_take_over: true,
        launch_at_login: false,
        ..Config::default()
    };
    let speakers = mac_studio_speakers();
    let snap = DeviceSnapshot { devices: vec![speakers.clone()], default_output: Some(speakers.id), ..Default::default() };
    let h = Harness::new(&config, snap, Opts::default());

    assert_eq!(h.rule("outputPin"), "on (no priority list, blocked: Maono AI Microphone)");
    assert_eq!(h.rule("inputPin"), "off");
    assert_eq!(
        h.rule("headphones"),
        "on (bluetooth output takes over when it connects; blocked from taking over: Maono AI Microphone)"
    );
}

#[test]
fn wildcard_volume_status_leads_with_the_default() {
    let config = Config {
        input_volume: volumes(&[("*", 100.0), ("Brio 100", 75.0), ("Wireless microphone", 88.0)]),
        ..pinning_with_liveness()
    };
    let snap = input_snapshot(vec![brio(), air_pods()], &[(brio().id, 0.75), (air_pods().id, 0.97)]);
    let h = Harness::new(&config, snap, Opts::default());

    assert_eq!(
        h.rule("inputVolume"),
        "on (default 100%; AirPods Max 100% (now 97%), Brio 100 75% (now 75%), Wireless microphone 88% (absent))"
    );
    assert_eq!(h.writes(), vec![format!("volume:{}:1.00", air_pods().id)]);
}

#[test]
fn same_named_devices_both_report_the_override() {
    let config = Config { input_volume: volumes(&[("*", 100.0), ("Brio 100", 75.0)]), ..pinning_with_liveness() };
    let snap = input_snapshot(vec![brio(), second_brio()], &[(brio().id, 0.75), (second_brio().id, 0.60)]);
    let h = Harness::new(&config, snap, Opts::default());

    assert_eq!(h.rule("inputVolume"), "on (default 100%; Brio 100 75% (now 75%), Brio 100 75% (now 60%))");
    assert_eq!(h.writes(), vec![format!("volume:{}:0.75", second_brio().id)]);
}

#[test]
fn present_device_with_no_reading_is_unreadable_not_absent() {
    let config = Config {
        input_volume: volumes(&[("*", 100.0), ("Brio 100", 75.0), ("Wireless microphone", 88.0)]),
        ..pinning_with_liveness()
    };
    let snap = input_snapshot(vec![brio(), air_pods()], &[(air_pods().id, 0.97)]);
    let h = Harness::new(&config, snap, Opts::default());

    assert_eq!(
        h.rule("inputVolume"),
        "on (default 100%; AirPods Max 100% (now 97%), Brio 100 75% (unreadable), Wireless microphone 88% (absent))"
    );
    assert_eq!(h.writes(), vec![format!("volume:{}:1.00", air_pods().id)]);
}

#[test]
fn named_only_volume_status_is_unchanged() {
    let config =
        Config { input_volume: volumes(&[("Brio 100", 75.0), ("Wireless microphone", 88.0)]), ..pinning_with_liveness() };
    let snap = input_snapshot(vec![brio(), air_pods()], &[(brio().id, 0.75), (air_pods().id, 0.97)]);
    let h = Harness::new(&config, snap, Opts::default());

    assert_eq!(h.rule("inputVolume"), "on (Brio 100 75% (now 75%), Wireless microphone 88% (absent))");
    assert!(h.writes().is_empty());
}
