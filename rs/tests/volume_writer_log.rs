//! The writer line parser, fed the text coreaudiod logged on 2026-10-07 (Swift
//! `VolumeWriterLogTests.swift`, 5 tests), plus the §6.5 match table rows Swift's landed rule keeps.

mod common;

use std::time::UNIX_EPOCH;

use cleat_rs::outvol::writer_log::{executable_path, parse_ndjson, VolumeWriterEvent};
use cleat_rs::rules::output_volume_hold::listed_entry;
use common::s;

const PARALLELS_LINE: &str = r#"{"timezoneName":"","messageType":"Default","eventType":"logEvent","source":null,"formatString":"Volume update from PID = %d Control ID = %d Scalar volume %f -> %f Element = %d","subsystem":"com.apple.bluetooth","category":"BTAudio","processImagePath":"\/usr\/sbin\/coreaudiod","senderImagePath":"\/System\/Library\/Audio\/Plug-Ins\/HAL\/BTAudioHALPlugin.driver\/Contents\/MacOS\/BTAudioHALPlugin","timestamp":"2026-10-07 10:19:55.713745+0800","eventMessage":"Volume update from PID = 8484 Control ID = 262 Scalar volume 0.488189 -> 0.427210 Element = 0","processID":342}"#;

fn ndjson(message: &str) -> String {
    format!(r#"{{"timestamp":"2026-10-07 10:19:55.713745+0800","eventMessage":"{message}"}}"#)
}

#[test]
fn parses_the_real_parallels_line() {
    let Some(VolumeWriterEvent::Write(w)) = parse_ndjson(PARALLELS_LINE, |pid| format!("name of {pid}")) else {
        panic!("not a write")
    };
    assert_eq!(w.pid, 8484);
    assert_eq!(w.writer, "name of 8484");
    assert_eq!(w.control, 262);
    assert!((w.from - 0.488189).abs() < 1e-6);
    assert!((w.to - 0.427210).abs() < 1e-6);
    let secs = w.at.duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
    assert!((secs - 1_791_339_595.713).abs() < 0.001, "{secs}");
    assert!(w.changes_value());
}

#[test]
fn header_and_blank_lines_are_not_events_but_foreign_wording_is_unrecognised() {
    assert_eq!(parse_ndjson(r#"Filtering the log data using "process == \"coreaudiod\"""#, |_| String::new()), None);
    assert_eq!(parse_ndjson("", |_| String::new()), None);
    let crown = ndjson("A2DP : Volume received from bluetoothd: volume 0.488189");
    assert_eq!(
        parse_ndjson(&crown, |_| String::new()),
        Some(VolumeWriterEvent::Unrecognised("A2DP : Volume received from bluetoothd: volume 0.488189".into()))
    );
}

#[test]
fn one_changed_word_makes_the_line_unrecognised() {
    let message = "Volume update from PID = 8484 Control ID = 262 Sclar volume 0.488189 -> 0.427210 Element = 0";
    assert_eq!(parse_ndjson(&ndjson(message), |_| String::new()), Some(VolumeWriterEvent::Unrecognised(message.into())));
}

#[test]
fn executable_paths() {
    let me = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert_eq!(executable_path(std::process::id() as i32), me.display().to_string());
    assert_eq!(executable_path(99_999_999), "pid 99999999");
}

#[test]
fn listed_entry_matches_executable_or_app() {
    let parallels = "/Applications/Parallels Desktop.app/Contents/MacOS/Parallels VM.app/Contents/MacOS/prl_vm_app";
    let chrome = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/141.0/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)";
    let m = |path: &str, entries: &[&str]| listed_entry(path, &s(entries));
    assert_eq!(m(parallels, &["prl_vm_app"]).as_deref(), Some("prl_vm_app"));
    assert_eq!(m(parallels, &["Parallels Desktop"]).as_deref(), Some("Parallels Desktop"));
    assert_eq!(m(parallels, &["Parallels VM"]).as_deref(), Some("Parallels VM"));
    assert_eq!(m(chrome, &["Jetto", "Google Chrome"]).as_deref(), Some("Google Chrome"));
    assert_eq!(m(parallels, &["Parallels"]), None);
    assert_eq!(m(parallels, &["MacOS"]), None);
    assert_eq!(m(parallels, &["prl_vm"]), None);
    assert_eq!(m(parallels, &[""]), None);
    assert_eq!(m("/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter", &["Parallels Desktop", "prl_vm_app"]), None);
    assert_eq!(m("pid 8484", &["Parallels Desktop"]), None);
    // §6.5 M3, M7, M8: list order wins, case counts, an entry is not written with ".app".
    assert_eq!(m(parallels, &["prl_vm_app", "Parallels Desktop"]).as_deref(), Some("prl_vm_app"));
    assert_eq!(m(parallels, &["parallels desktop"]), None);
    assert_eq!(m(parallels, &["Parallels Desktop.app"]), None);
}
