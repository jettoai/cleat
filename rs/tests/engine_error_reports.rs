//! EngineTests "Error reports" (Swift EngineTests.swift :804-:848), test for test.

mod common;

use cleat_rs::config::Config;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};

fn run(json: &str) -> Harness {
    Harness::with_json(json, DeviceSnapshot::default(), Opts::default())
}

#[test]
fn error_reports_off_never_reaches_the_reporter() {
    let h = run(r#"{"launchAtLogin": false}"#);
    assert!(h.reports.borrow().is_empty());
    assert_eq!(h.status().error_reports, Some(false));
    assert!(!h.log_lines().iter().any(|l| l.contains("errorReports:")));
}

#[test]
fn error_reports_on_at_start_reach_the_reporter_once() {
    let h = run(r#"{"launchAtLogin": false, "errorReports": true}"#);
    assert_eq!(*h.reports.borrow(), vec![true]);
    assert_eq!(h.status().error_reports, Some(true));
    assert!(h.log_lines().iter().any(|l| l.ends_with("errorReports: on")));
}

#[test]
fn error_reports_follow_the_config_both_ways() {
    let mut h = run(r#"{"launchAtLogin": false, "errorReports": true}"#);
    let off = Config { launch_at_login: false, error_reports: false, ..Config::default() };
    std::fs::write(h.dir.join("config.json"), serde_json::to_string(&off).unwrap()).unwrap();
    h.engine.handle(cleat_rs::engine::Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
    // A second sync with nothing changed reaches nobody.
    h.engine.handle(cleat_rs::engine::Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
    assert_eq!(*h.reports.borrow(), vec![true, false]);
}
