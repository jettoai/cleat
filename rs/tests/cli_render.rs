//! `status` and `log` against the Swift build's real output: the fixtures were captured from
//! `/Applications/Cleat.app` 0.3.8 (`Cleat status`, `Cleat log -n 7`) at the same moment as the
//! status.json and log they were rendered from. The oracle is Swift's output, not strings written
//! here.

use std::path::Path;

use cleat_rs::cli::{render_log, render_status};
use cleat_rs::engine::Status;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn status_matches_swift_byte_for_byte() {
    // The golden's `updated:` line is local time in Taipei.
    std::env::set_var("TZ", "Asia/Taipei");
    // SAFETY: tzset re-reads TZ; this test binary has no other thread reading time zones.
    unsafe { libc_tzset() };

    let status: Status = serde_json::from_str(&fixture("swift-status.json")).unwrap();
    let (text, code) = render_status(
        Some(&status),
        true,
        "launchd agent (registered)",
        "~/Library/Application Support/Cleat/status.json",
        "~/.config/cleat/config.json",
    );
    // 0.3.8 predates the `reaction:` line; a newer Swift prints `-` for a file without `performance`
    // (CLI.swift `printVitals`; its `vitals:` line is not ported).
    let golden = fixture("swift-status.txt").replacen("\nrules:", "\nreaction:    -\nrules:", 1);
    assert_eq!(text, golden);
    assert_eq!(code, 0);
}

#[test]
fn stopped_daemon_and_missing_file() {
    let status: Status = serde_json::from_str(&fixture("swift-status.json")).unwrap();
    let (text, code) = render_status(Some(&status), false, "x", "", "c");
    assert!(text.starts_with("daemon:      not running (last seen as pid 50568)\nsupervision: x\n"));
    assert_eq!(code, 1);

    let (text, code) = render_status(None, false, "launchd agent (not registered)", "~/s.json", "c");
    assert_eq!(text, "daemon:      not running (no status file at ~/s.json)\nsupervision: launchd agent (not registered)\n");
    assert_eq!(code, 1);
}

#[test]
fn log_matches_swift_byte_for_byte() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swift-cleat.log");
    assert_eq!(render_log(7, &path), fixture("swift-log7.txt"));
}

#[test]
fn log_without_events_names_the_file() {
    let path = std::env::temp_dir().join("cleat-rs-no-such-log.log");
    assert!(render_log(50, &path).starts_with("no events yet ("));
}

extern "C" {
    #[link_name = "tzset"]
    fn libc_tzset();
}

fn render_with_photos(block: &str) -> String {
    let mut status: serde_json::Value = serde_json::from_str(&fixture("swift-status.json")).unwrap();
    status["photos"] = serde_json::from_str(block).unwrap();
    let status: Status = serde_json::from_value(status).unwrap();
    render_status(Some(&status), true, "x", "s", "c").0
}

fn photos_line(text: &str) -> &str {
    text.lines().find(|l| l.starts_with("photos:")).unwrap_or("none")
}

#[test]
fn status_has_a_photos_line_for_each_state() {
    let off = render_with_photos(r#"{"state":"off","converted":0}"#);
    assert_eq!(photos_line(&off), "photos:      off");
    let none_yet = render_with_photos(r#"{"state":"watching","converted":0}"#);
    assert_eq!(photos_line(&none_yet), "photos:      watching, 0 converted");
    let watching = render_with_photos(r#"{"state":"watching","converted":3,"last":{"name":"IMG_5708.HEIC","ms":187}}"#);
    assert_eq!(photos_line(&watching), "photos:      watching, 3 converted, last IMG_5708.HEIC 187 ms");
    let denied = render_with_photos(r#"{"state":"no-access","converted":1}"#);
    assert_eq!(photos_line(&denied), "photos:      no-access, 1 converted");
}

#[test]
fn a_status_file_without_photos_prints_no_photos_line() {
    let status: Status = serde_json::from_str(&fixture("swift-status.json")).unwrap();
    assert!(!render_status(Some(&status), true, "x", "s", "c").0.contains("photos:"));
}
