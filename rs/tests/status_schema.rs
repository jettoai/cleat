//! status.json: the Swift daemon's file reads, an older daemon's file reads, and the Rust daemon
//! writes the same top-level keys the Swift one does.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use cleat_rs::config::Config;
use cleat_rs::engine::Status;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};
use common::*;

fn swift_json() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swift-status.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn keys(v: &serde_json::Value) -> BTreeSet<String> {
    v.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn reads_the_swift_daemons_file() {
    let s: Status = serde_json::from_value(swift_json()).unwrap();
    assert_eq!(s.pid, 50568);
    assert_eq!(s.error_reports, Some(true));
    assert_eq!(s.microphone, "authorized");
}

/// Swift `testStatusFromAnOlderDaemonStillReads`.
#[test]
fn status_from_an_older_daemon_still_reads() {
    let s: Status = serde_json::from_str(
        r#"{"pid": 123, "updatedAt": "2026-10-01T00:00:00Z", "configState": "ok",
            "microphone": "authorized", "rules": {}, "liveness": {}, "recentEvents": []}"#,
    )
    .unwrap();
    assert_eq!(s.pid, 123);
    assert_eq!(s.error_reports, None);
}

#[test]
fn rust_writes_the_swift_key_set() {
    let config = Config { error_reports: true, launch_at_login: false, ..pinned_input() };
    let snap = DeviceSnapshot {
        devices: vec![brio(), mac_studio_speakers()],
        default_input: Some(brio().id),
        default_output: Some(mac_studio_speakers().id),
        ..Default::default()
    };
    let h = Harness::new(&config, snap, Opts::default());
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(h.dir.join("status.json")).unwrap()).unwrap();
    // Swift 0.3.11's own status.json (2026-10-07, recentEvents emptied): 0.3.10 added outputVolume.
    let current: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/swift-status-0.3.11.json")).unwrap();
    // The Swift build after 0.3.11 (ui-settings-direction) adds `performance`.
    let mut expected = keys(&current);
    expected.insert("performance".into());
    // The "not used" rule (B-1287) adds `stuck`: the device a side is left on, or null.
    expected.insert("stuck".into());
    assert_eq!(keys(&written), expected);
}
