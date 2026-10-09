//! The output volume hold inside a real engine (Swift `OutputVolumeEngineTests.swift`, 11 tests),
//! plus §6.6 E6. `drive(ms)` is Swift's `waitOnQueue`.

mod common;

use cleat_rs::audio::ListenerKind;
use cleat_rs::config::Config;
use cleat_rs::engine::Event;
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::outvol::writer_log::{VolumeWrite, VolumeWriterEvent};
use cleat_rs::state::clock::Clock;
use common::engine::{Harness, Opts};
use common::*;

const PARALLELS: &str = "/Applications/Parallels Desktop.app/Contents/MacOS/Parallels VM.app/Contents/MacOS/prl_vm_app";
const CONTROL_CENTER: &str = "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter";

fn start_with(hold: &[&str], balance: Option<f64>, starts: bool, enabled: bool) -> Harness {
    let config = Config {
        balance,
        launch_at_login: false,
        output_volume_hold_against: s(hold),
        output_volume_hold_enabled: enabled,
        ..Config::default()
    };
    let snap = DeviceSnapshot {
        devices: vec![air_pods()],
        default_output: Some(air_pods().id),
        output_balance: Some(0.5),
        output_volumes: vec![0.5, 0.5],
        ..Default::default()
    };
    let h = Harness::new(&config, snap, Opts { writer_starts: starts, ..Opts::default() });
    assert!(h.writes().is_empty());
    h
}

fn start(hold: &[&str]) -> Harness {
    start_with(hold, None, true, true)
}

fn emit(h: &mut Harness, path: &str, from: f32, to: f32, pid: i32, control: i64) {
    let at = h.clock.wall();
    h.engine.handle(Event::VolumeWriter(VolumeWriterEvent::Write(VolumeWrite {
        at,
        pid,
        writer: path.into(),
        control,
        from,
        to,
    })));
}

fn output_reads(h: &mut Harness, values: &[f32]) {
    h.set(|s| s.output_volumes = values.to_vec());
    h.engine.handle(Event::OutputVolumeChanged { device: air_pods().id });
}

fn status_block(h: &Harness) -> serde_json::Value {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(h.dir.join("status.json")).unwrap()).unwrap();
    v["outputVolume"].clone()
}

#[test]
fn listed_app_write_is_reverted_and_reported() {
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, PARALLELS, 0.5, 0.43, 8484, 262);
    output_reads(&mut h, &[0.43, 0.43]);
    h.drive(600);
    assert_eq!(h.writes(), s(&["outvol:30:0.5000"]));
    let log = h.log_lines();
    assert!(log.iter().any(|l| l.ends_with("outputVolume: AirPods Max 50% -> 43% writer=Parallels Desktop reverted")));
    assert!(log.iter().any(|l| l.ends_with("outputVolume: AirPods Max 43% -> 50% (reverted Parallels Desktop)")));
    let line = h.rule("outputVolume");
    assert!(line.starts_with("on (holding against Parallels Desktop); last revert "), "{line}");
    let block = status_block(&h);
    let keys: Vec<&String> = block.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["lastRevert", "lastRevertDetail", "state"]);
    assert_eq!(block["state"], "on (holding against Parallels Desktop)");
    let revert = block["lastRevert"].as_str().unwrap();
    let (clock, rest) = revert.split_at(5);
    assert!(clock.chars().enumerate().all(|(i, c)| if i == 2 { c == ':' } else { c.is_ascii_digit() }), "{revert}");
    assert_eq!(rest, " 拉回 Parallels Desktop 改的音量 43% → 50%");
}

/// B-1283: the window words `lastRevertDetail` itself; in Chinese it reads exactly as the daemon's
/// own `lastRevert`, so an older window and a newer one agree. 0.125 sits on a rounding tie (12.5%),
/// which only an unrounded `from` renders as `lastRevert` does.
#[test]
fn revert_detail_reads_as_the_legacy_line() {
    use cleat_rs::settings::lang::Lang;
    use cleat_rs::settings::sources::{last_revert, LastRevert};
    use cleat_rs::settings::text::revert_line;
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, PARALLELS, 0.5, 0.125, 8484, 262);
    output_reads(&mut h, &[0.125, 0.125]);
    h.drive(600);
    let data = std::fs::read(h.dir.join("status.json")).unwrap();
    let legacy = status_block(&h)["lastRevert"].as_str().unwrap().to_string();
    let r = last_revert(&data).unwrap();
    assert!(matches!(r, LastRevert::Detail(_)), "{r:?}");
    assert_eq!(revert_line(Some(&r), Lang::ZhHant), format!("最近一次：{legacy}"));
    let clock = &legacy[..5];
    assert_eq!(revert_line(Some(&r), Lang::En), format!("Last undo {clock}: Parallels Desktop set 12%, put back to 50%"));
    let old = br#"{"outputVolume":{"lastRevert":"13:40 \u62c9\u56de x"}}"#;
    assert_eq!(revert_line(last_revert(old).as_ref(), Lang::En), "Last undo at 13:40");
}

#[test]
fn executable_name_entry_still_matches() {
    let mut h = start(&["prl_vm_app"]);
    emit(&mut h, PARALLELS, 0.5, 0.43, 8484, 262);
    output_reads(&mut h, &[0.43, 0.43]);
    h.drive(600);
    assert_eq!(h.writes(), s(&["outvol:30:0.5000"]));
    assert!(h.log_lines().iter().any(|l| l.ends_with("(reverted prl_vm_app)")));
}

#[test]
fn unlisted_writer_is_kept_and_logged_once() {
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, CONTROL_CENTER, 0.5, 0.43, 531, 262);
    output_reads(&mut h, &[0.43, 0.43]);
    h.drive(600);
    h.engine.handle(Event::OutputVolumeChanged { device: air_pods().id });
    h.drive(500);
    assert!(h.writes().is_empty());
    let writer_lines: Vec<String> = h.log_lines().into_iter().filter(|l| l.contains("writer=")).collect();
    assert_eq!(writer_lines.len(), 1);
    assert!(writer_lines[0].ends_with("outputVolume: AirPods Max 50% -> 43% writer=ControlCenter kept"));
    assert_eq!(status_block(&h), serde_json::json!({"state": "on (holding against Parallels Desktop)"}));
}

#[test]
fn listed_write_to_another_control_leaves_an_unmoved_output_alone() {
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, PARALLELS, 1.0, 0.5, 8484, 261);
    h.drive(600);
    assert!(h.writes().is_empty());
}

#[test]
fn microphone_write_before_an_output_write_restores_the_outputs_value() {
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, PARALLELS, 1.0, 0.4, 8484, 261);
    h.drive(100);
    emit(&mut h, PARALLELS, 0.5, 0.4, 8484, 262);
    output_reads(&mut h, &[0.4, 0.4]);
    h.drive(600);
    assert_eq!(h.writes(), s(&["outvol:30:0.5000"]));
}

#[test]
fn unrecognised_line_during_a_pending_write_stops_the_revert() {
    let mut h = start(&["Parallels Desktop"]);
    emit(&mut h, PARALLELS, 0.5, 0.43, 8484, 262);
    output_reads(&mut h, &[0.43, 0.43]);
    h.engine.handle(Event::VolumeWriter(VolumeWriterEvent::Unrecognised("volume changed by something new".into())));
    h.drive(600);
    assert!(h.writes().is_empty());
    assert!(h.log_lines().iter().any(|l| l.ends_with("writer=Parallels Desktop kept (paused)")));
}

#[test]
fn source_that_cannot_start_pauses_the_hold() {
    let mut h = start_with(&["Parallels Desktop"], None, false, true);
    assert_eq!(h.rule("outputVolume"), "paused: cannot read system log writer (log stream could not start)");
    output_reads(&mut h, &[0.43, 0.43]);
    h.drive(600);
    assert!(h.writes().is_empty());
}

#[test]
fn source_that_exits_is_restarted() {
    let mut h = start(&["Parallels Desktop"]);
    assert_eq!(h.writer.starts.get(), 1);
    h.engine.handle(Event::VolumeWriterExited(1));
    h.reconcile();
    assert_eq!(h.rule("outputVolume"), "paused: cannot read system log writer (log stream exited (1))");
    assert!(h.log_lines().iter().any(|l| l.ends_with("outputVolume: writer log stopped (exit 1), restarting in 1s")));
    h.drive(1200);
    assert_eq!(h.writer.starts.get(), 2);
    h.reconcile();
    assert_eq!(h.rule("outputVolume"), "on (holding against Parallels Desktop)");
}

#[test]
fn empty_list_never_starts_the_source() {
    let mut h = start(&[]);
    output_reads(&mut h, &[0.43, 0.43]);
    h.drive(500);
    assert_eq!(h.writer.made.get(), 0);
    assert_eq!(h.rule("outputVolume"), "off");
    assert!(h.writes().is_empty());
}

/// Parallels writes one channel, then the other 240ms later: the hold puts the volume back first,
/// and the balance rule finds nothing to do.
#[test]
fn revert_and_balance_do_not_fight() {
    let mut h = start_with(&["Parallels Desktop"], Some(0.5), true, true);
    emit(&mut h, PARALLELS, 0.5, 0.43, 8484, 262);
    h.set(|s| s.output_balance = Some(0.46));
    output_reads(&mut h, &[0.43, 0.5]);
    h.fire(ListenerKind::Balance);
    h.drive(240);
    emit(&mut h, PARALLELS, 0.5, 0.43, 8484, 263);
    h.set(|s| s.output_balance = Some(0.5));
    output_reads(&mut h, &[0.43, 0.43]);
    h.fire(ListenerKind::Balance);
    h.drive(760);
    assert_eq!(h.writes(), s(&["outvol:30:0.5000"]));
}

/// After the output moved to the speakers (40%), the AirPods' listener delivers one last reading
/// (80%). It is not the speakers' baseline.
#[test]
fn late_reading_from_the_previous_output_is_dropped() {
    let mut h = start(&["Parallels Desktop"]);
    let speakers = mac_speakers();
    h.set(|s| {
        s.devices.push(speakers.clone());
        s.default_output = Some(speakers.id);
        s.output_volumes = vec![0.4, 0.4];
    });
    h.reconcile();
    h.set(|s| s.output_volumes = vec![0.8, 0.8]);
    h.engine.output_volume_changed(air_pods().id);
    h.set(|s| s.output_volumes = vec![0.4, 0.4]);
    emit(&mut h, PARALLELS, 0.8, 0.4, 8484, 261);
    h.engine.output_volume_changed(speakers.id);
    h.drive(600);
    assert!(h.writes().is_empty());
}

/// §6.6 E6: the switch off never makes a source; switched on again, one is made.
#[test]
fn hold_switched_off_keeps_the_list_and_starts_nothing() {
    let mut h = start_with(&["Parallels Desktop"], None, true, false);
    assert_eq!(h.writer.made.get(), 0);
    assert_eq!(h.rule("outputVolume"), "off");
    assert_eq!(status_block(&h)["state"], "off");
    let config = Config {
        launch_at_login: false,
        output_volume_hold_against: s(&["Parallels Desktop"]),
        ..Config::default()
    };
    std::fs::write(h.dir.join("config.json"), serde_json::to_string(&config).unwrap()).unwrap();
    h.engine.handle(Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
    assert_eq!(h.writer.made.get(), 1);
}
