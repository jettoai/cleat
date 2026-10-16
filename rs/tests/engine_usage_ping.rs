//! EngineTests "usagePing" (Swift EngineTests.swift :942-:978).

mod common;

use cleat_rs::config::Config;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};

fn run(json: &str) -> Harness {
    Harness::with_json(json, DeviceSnapshot::default(), Opts::default())
}

#[test]
fn usage_ping_off_in_the_config_never_reaches_the_pinger() {
    let h = run(r#"{"launchAtLogin": false, "usagePing": false}"#);
    assert!(h.pings.borrow().is_empty());
    assert!(!h.log_lines().iter().any(|l| l.contains("usagePing:")));
}

#[test]
fn usage_ping_is_on_at_start_and_follows_the_config_both_ways() {
    // No key at all: on.
    let mut h = run(r#"{"launchAtLogin": false}"#);
    assert_eq!(*h.pings.borrow(), vec![true]);
    assert_eq!(h.log_lines().iter().filter(|l| l.ends_with("usagePing: on")).count(), 1);

    let touch = |h: &mut Harness, c: &Config| {
        std::fs::write(h.dir.join("config.json"), serde_json::to_string(c).unwrap()).unwrap();
        h.engine.handle(cleat_rs::engine::Event::ConfigTouched { received: std::time::SystemTime::now() });
        h.drive(400);
    };
    let off = Config { launch_at_login: false, usage_ping: false, ..Config::default() };
    touch(&mut h, &off);
    touch(&mut h, &Config { launch_at_login: false, usage_ping: false, error_reports: true, ..Config::default() });
    let on = Config { launch_at_login: false, ..Config::default() };
    touch(&mut h, &on);
    assert_eq!(*h.pings.borrow(), vec![true, false, true]);
    assert_eq!(h.log_lines().iter().filter(|l| l.ends_with("usagePing: off")).count(), 1);
}
