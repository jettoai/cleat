//! B-1283 return: the engine side of evicting a blocked default. The device cleat moved to is not a
//! hand pick, and a device macOS keeps putting back is evicted at most three times a minute.

mod common;

use std::time::{Duration, SystemTime};

use cleat_rs::config::Config;
use cleat_rs::engine::Event;
use cleat_rs::model::{AudioDevice, DeviceSnapshot};
use common::engine::{Harness, Opts};
use common::*;

/// Albert's input list without silence detection, so presence alone decides.
fn input_config() -> Config {
    Config { liveness: Default::default(), ..pinned_input() }
}

fn inputs(devices: Vec<AudioDevice>, current: &AudioDevice) -> DeviceSnapshot {
    DeviceSnapshot { devices, default_input: Some(current.id), ..Default::default() }
}

fn count(h: &Harness, write: &str) -> usize {
    h.writes().iter().filter(|w| *w == write).count()
}

fn reload(h: &mut Harness, config: &Config) {
    std::fs::write(h.dir.join("config.json"), serde_json::to_string(config).unwrap()).unwrap();
    h.engine.handle(Event::ConfigTouched { received: SystemTime::now() });
    h.drive(400);
}

/// macOS puts AirPods Max back as the default input `n` times, half a second apart.
fn air_pods_come_back(h: &mut Harness, n: usize) {
    for _ in 0..n {
        h.set(|s| s.default_input = Some(air_pods().id));
        h.advance(Duration::from_millis(500));
        h.reconcile();
    }
}

#[test]
fn evicted_to_built_in_gives_way_when_the_listed_mic_returns() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    assert_eq!(h.writes(), vec!["input:81"]);
    h.set(|s| s.devices.push(wireless()));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:81", "input:10"]);
}

#[test]
fn hand_picked_built_in_is_left_alone() {
    let mut h = Harness::new(&input_config(), inputs(vec![mac_mic(), wireless()], &mac_mic()), Opts::default());
    h.reconcile();
    assert_eq!(h.writes(), Vec::<String>::new());
}

#[test]
fn placed_mark_clears_once_the_user_picks_another() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    h.set(|s| {
        s.devices.push(iphone_mic());
        s.default_input = Some(iphone_mic().id);
    });
    h.reconcile();
    h.set(|s| {
        s.default_input = Some(mac_mic().id);
        s.devices.push(wireless());
    });
    h.reconcile();
    assert_eq!(count(&h, "input:10"), 0, "{:?}", h.writes());
}

/// Albert's replay (b): the only listed output is the wired headphones, then he marks it "not used".
#[test]
fn not_used_on_the_only_listed_output_moves_off_it() {
    let config = Config { output: s(&["外接耳機"]), ..Config::default() };
    let snap = DeviceSnapshot {
        devices: vec![wired_headphones(), mac_speakers()],
        default_output: Some(wired_headphones().id),
        ..Default::default()
    };
    let mut h = Harness::new(&config, snap, Opts::default());
    assert_eq!(h.writes(), Vec::<String>::new());
    reload(&mut h, &Config { output: vec![], blocked_output: s(&["外接耳機"]), ..Config::default() });
    assert_eq!(h.writes(), vec!["output:70"]);
}

#[test]
fn evictions_pause_after_three_in_a_minute() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    air_pods_come_back(&mut h, 5);
    assert_eq!(count(&h, "input:81"), 3, "{:?}", h.writes());
    assert_eq!(h.count("keeps coming back"), 1);
}

#[test]
fn a_device_change_lifts_the_pause() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    air_pods_come_back(&mut h, 5);
    h.set(|s| s.devices.push(zoom()));
    h.reconcile();
    assert_eq!(count(&h, "input:81"), 4, "{:?}", h.writes());
}

#[test]
fn a_config_reload_lifts_the_pause() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    air_pods_come_back(&mut h, 5);
    reload(&mut h, &Config { blocked_output: s(&["Studio Display Speakers"]), ..input_config() });
    assert_eq!(count(&h, "input:81"), 4, "{:?}", h.writes());
}

#[test]
fn the_pause_holds_past_the_window_without_a_device_change() {
    let mut h = Harness::new(&input_config(), inputs(vec![air_pods(), mac_mic()], &air_pods()), Opts::default());
    air_pods_come_back(&mut h, 5);
    h.advance(Duration::from_secs(61));
    air_pods_come_back(&mut h, 1);
    assert_eq!(count(&h, "input:81"), 3, "{:?}", h.writes());
}

#[test]
fn output_side_has_its_own_cooldown() {
    let config = Config { blocked_output: s(&["AirPods Max"]), ..Config::default() };
    let snap = DeviceSnapshot {
        devices: vec![air_pods(), mac_speakers()],
        default_output: Some(air_pods().id),
        ..Default::default()
    };
    let mut h = Harness::new(&config, snap, Opts::default());
    for _ in 0..5 {
        h.set(|s| s.default_output = Some(air_pods().id));
        h.advance(Duration::from_millis(500));
        h.reconcile();
    }
    assert_eq!(count(&h, "output:70"), 3, "{:?}", h.writes());
    assert_eq!(h.count("keeps coming back"), 1);
}
