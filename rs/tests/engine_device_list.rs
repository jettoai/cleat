//! B-2481: a device list the HAL would not give is not an empty one. Same fixtures as
//! `engine_arrivals.rs` (Albert's Mac Studio speakers and AirPods Max); the unreadable pass is
//! injected through `DeviceSnapshot::devices_unreadable`, the real HAL failure was never captured.

mod common;

use cleat_rs::config::Config;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};
use common::reclaim::*;
use common::*;

const UNREADABLE: &str = "devices: list unreadable, pass skipped";

fn headphones_config() -> Config {
    Config {
        output: s(&["Mac Studio的揚聲器"]),
        blocked_output: s(&["Maono AI Microphone"]),
        headphones_take_over: true,
        launch_at_login: false,
        ..Config::default()
    }
}

/// Both devices present, the speakers chosen by hand: nothing to do.
fn settled() -> Harness {
    let speakers = mac_studio_speakers();
    let snap = DeviceSnapshot {
        devices: vec![speakers.clone(), air_pods()],
        default_output: Some(speakers.id),
        ..Default::default()
    };
    let h = Harness::new(&headphones_config(), snap, Opts::default());
    assert!(h.writes().is_empty());
    h
}

fn unreadable(h: &Harness) {
    h.set(|s| {
        s.devices = vec![];
        s.devices_unreadable = true;
    });
}

fn readable(h: &Harness) {
    h.set(|s| {
        s.devices = vec![mac_studio_speakers(), air_pods()];
        s.devices_unreadable = false;
    });
}

#[test]
fn good_list_after_an_unreadable_pass_arrives_nothing() {
    let mut h = settled();
    unreadable(&h);
    h.engine.reconcile(true, None);
    readable(&h);
    h.engine.reconcile(true, None);
    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert_eq!(h.count("headphones connected"), 0);
}

#[test]
fn a_list_that_is_truly_empty_still_counts_as_every_device_leaving() {
    let mut h = settled();
    h.set(|s| s.devices = vec![]);
    h.engine.reconcile(true, None);
    assert_eq!(h.count(UNREADABLE), 0);
    readable(&h);
    h.engine.reconcile(true, None);
    assert_eq!(h.writes(), vec![format!("output:{}", air_pods().id)]);
}

#[test]
fn a_long_outage_is_one_log_line_and_a_new_outage_is_another() {
    let mut h = settled();
    for _ in 0..3 {
        unreadable(&h);
        h.engine.reconcile(true, None);
    }
    assert_eq!(h.count(UNREADABLE), 1);
    readable(&h);
    h.engine.reconcile(true, None);
    assert_eq!(h.count(UNREADABLE), 1);
    unreadable(&h);
    h.engine.reconcile(true, None);
    assert_eq!(h.count(UNREADABLE), 2);
}

#[test]
fn an_unreadable_pass_asks_for_no_headset_and_sets_no_throttle() {
    // AirPods are the output and the Mac is playing: reclaim has nothing to ask for.
    let mut w = world_where(Some(already_routed()), |s| {
        s.devices = vec![air_pods(), mac_studio_speakers()];
        s.default_output = Some(air_pods().id);
    });
    assert!(w.requests().is_empty());

    // The same list read as empty would show the headset gone, and reclaim would ask and arm its throttle.
    w.set(|s| {
        s.devices = vec![];
        s.devices_unreadable = true;
    });
    w.engine.reconcile(true, None);
    assert!(w.requests().is_empty(), "{:?}", w.requests());

    // A real departure right after is asked for at once: no cooldown was left behind.
    w.set(|s| {
        s.devices = vec![mac_studio_speakers()];
        s.default_output = Some(mac_studio_speakers().id);
        s.devices_unreadable = false;
    });
    w.engine.reconcile(true, None);
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}
