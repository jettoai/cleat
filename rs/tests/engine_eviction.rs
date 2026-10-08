//! B-1283 return: the engine side of evicting a blocked default. The device cleat moved to is not a
//! hand pick, and a device macOS keeps putting back is evicted at most three times a minute.

mod common;

use std::time::{Duration, SystemTime};

use cleat_rs::audio::ListenerKind;
use cleat_rs::config::Config;
use cleat_rs::engine::{Event, Status};
use cleat_rs::model::{AudioDevice, DeviceSnapshot};
use cleat_rs::model::MicrophonePermission;
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

/// Albert's replay (a2) end to end: the wireless microphone is silent, so AirPods Max goes to the
/// built-in microphone; once the wireless one has signal, cleat hands the input back to it.
#[test]
fn silent_wireless_microphone_takes_over_from_the_built_in_once_it_has_signal() {
    let config = Config { launch_at_login: false, ..pinned_input() };
    let snap = inputs(vec![air_pods(), wireless(), mac_mic()], &wireless());
    let opts = Opts { microphone: MicrophonePermission::Granted, start_results: vec![true], ..Opts::default() };
    let mut h = Harness::new(&config, snap, opts);
    let flip = |live| Event::LivenessFlip { uid: wireless().uid, name: wireless().name, live };
    h.engine.handle(flip(false));
    h.set(|s| s.default_input = Some(air_pods().id));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:81"]);
    h.engine.handle(flip(true));
    assert_eq!(h.writes(), vec!["input:81", "input:10"]);
}

// B-1287: the "not used" rule end to end on Albert's Mac Studio (PM 0209).

fn granted() -> Opts {
    Opts { microphone: MicrophonePermission::Granted, start_results: vec![true], ..Opts::default() }
}

fn stuck_input(h: &Harness) -> Option<String> {
    Status::read(&h.dir.join("status.json")).and_then(|s| s.stuck).and_then(|s| s.input)
}

/// MS-7: nothing else usable: the input stays, one log line, status.json says so; a device that
/// comes back ends it with one more line.
#[test]
fn stuck_input_is_written_to_status_and_logged_once() {
    let devices = vec![mac_studio_speakers(), teams(), zoom(), air_pods(), continuity_iphone()];
    let mut h = Harness::new(&albert_config(), inputs(devices, &air_pods()), Opts::default());
    h.reconcile();
    h.reconcile();
    assert_eq!(h.writes(), Vec::<String>::new());
    assert_eq!(h.count("pinInput: no other usable device, still on AirPods Max"), 1);
    assert_eq!(stuck_input(&h).as_deref(), Some("AirPods Max"));
    assert!(h.rule("inputPin").ends_with(", stuck on AirPods Max (no other usable device)"), "{}", h.rule("inputPin"));
    h.set(|s| s.devices.push(brio()));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:20"]);
    assert_eq!(h.count("pinInput: AirPods Max no longer the default"), 1);
    assert_eq!(stuck_input(&h), None);
}

/// E1 (PM 0209 #3): AirPods Max is held by the cooldown while the wireless microphone is silent;
/// the wireless microphone getting signal lifts the pause and takes the input.
#[test]
fn a_microphone_signal_change_lifts_the_pause() {
    let snap = inputs(vec![air_pods(), wireless(), mac_mic()], &wireless());
    let mut h = Harness::new(&pinned_input(), snap, granted());
    let flip = |live| Event::LivenessFlip { uid: wireless().uid, name: wireless().name, live };
    h.engine.handle(flip(false));
    air_pods_come_back(&mut h, 5);
    assert_eq!(h.writes(), vec!["input:81"; 3]);
    h.set(|s| s.default_input = Some(air_pods().id));
    h.engine.handle(flip(true));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:81", "input:81", "input:81", "input:10"]);
}

/// T-1: a headset arriving while the output is a "not used" device is takeover's, one write, and
/// is not counted against the cooldown.
#[test]
fn an_arriving_headset_off_a_blocked_output_is_one_write_and_no_strike() {
    let config = Config { blocked_output: s(&["Maono AI Microphone"]), headphones_take_over: true, ..Config::default() };
    let snap = DeviceSnapshot {
        devices: vec![maono_io(), mac_speakers()],
        default_output: Some(maono_io().id),
        ..Default::default()
    };
    let mut h = Harness::new(&config, snap, Opts::default());
    h.set(|s| {
        s.default_output = Some(maono_io().id);
        s.devices.push(air_pods());
    });
    h.fire(ListenerKind::Devices);
    h.drive(600);
    assert_eq!(h.writes(), vec!["output:70", "output:30"]);
    assert_eq!(h.count("keeps coming back"), 0);
}

/// PM ruling on B1: AirPods Max grabbed the microphone, the wireless one is there but silent, no
/// built-in microphone: the wireless one, and nothing more once it has signal.
#[test]
fn mac_studio_silent_wireless_microphone_is_the_escape_and_stays() {
    let mut h = Harness::new(&albert_config(), inputs(mac_studio(true, true, true, false), &wireless()), granted());
    let flip = |live| Event::LivenessFlip { uid: wireless().uid, name: wireless().name, live };
    h.engine.handle(flip(false));
    h.set(|s| s.default_input = Some(air_pods().id));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:10"]);
    h.engine.handle(flip(true));
    h.reconcile();
    assert_eq!(h.writes(), vec!["input:10"]);
}

/// PM ruling on B3: a meeting app switching the input to its own virtual device is left alone.
#[test]
fn a_meeting_app_on_its_blocked_virtual_device_is_left_alone() {
    let mut h = Harness::new(&albert_config(), inputs(mac_studio(true, false, true, false), &teams()), Opts::default());
    h.reconcile();
    assert_eq!(h.writes(), Vec::<String>::new());
    assert_eq!(stuck_input(&h), None);
}

/// MS-10: the replay the PM asked for, on Albert's machine and config.
#[test]
fn mac_studio_replay_end_to_end() {
    let snap = DeviceSnapshot {
        devices: mac_studio(true, true, false, false),
        default_input: Some(air_pods().id),
        default_output: Some(wired_headphones().id),
        ..Default::default()
    };
    let mut h = Harness::new(&albert_config(), snap, granted());
    let flip = |live| Event::LivenessFlip { uid: wireless().uid, name: wireless().name, live };
    // (a) AirPods Max grabbed the microphone, the wireless one is not plugged in.
    let a = h.writes();
    assert_eq!(stuck_input(&h).as_deref(), Some("AirPods Max"));
    // (b) the wireless microphone is plugged in and silent; macOS puts AirPods Max back.
    h.set(|s| s.devices.push(wireless()));
    h.reconcile();
    h.engine.handle(flip(false));
    h.set(|s| s.default_input = Some(air_pods().id));
    h.reconcile();
    let b = h.writes()[a.len()..].to_vec();
    // (c) the wireless microphone gets signal.
    h.engine.handle(flip(true));
    let c = h.writes()[a.len() + b.len()..].to_vec();
    // (d) he marks the wired headphones "not used" and takes them off the list.
    let mut config = albert_config();
    config.output = vec![];
    config.blocked_output.push("外接耳機".into());
    reload(&mut h, &config);
    let d = h.writes()[a.len() + b.len() + c.len()..].to_vec();
    eprintln!("MS-10 a={a:?} b={b:?} c={c:?} d={d:?}");
    assert_eq!(a, Vec::<String>::new());
    assert_eq!(stuck_input(&h), None);
    assert_eq!(b, vec!["input:10", "input:10"]);
    assert_eq!(c, Vec::<String>::new());
    assert_eq!(d, vec!["output:95"]);
}
