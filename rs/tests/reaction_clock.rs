//! Swift `PerformanceTests.swift`: the reaction clock, its status.json block and the numbers the
//! vitals band and `cleat status` show. Fixture values are Swift's.

mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use cleat_rs::audio::ListenerKind;
use cleat_rs::cli::render_status;
use cleat_rs::config::Config;
use cleat_rs::engine::Status;
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::settings::sources::performance;
use cleat_rs::settings::text::{millis_text, reaction_help, reaction_note, reaction_value};
use cleat_rs::settings::vitals::{DaemonVitals, VitalsState};
use cleat_rs::state::reaction_clock::{DaemonPerformance, Reaction, ReactionClock};
use common::engine::{Harness, Opts};
use common::*;

const DATE: SystemTime = UNIX_EPOCH;

fn totals(c: &ReactionClock) -> Vec<u64> {
    c.recent().iter().map(|r| r.total_nanos).collect()
}

#[test]
fn reaction_spans_event_to_last_write_of_the_pass() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 100);
    c.pass_began(600);
    c.wrote(605, DATE);
    c.wrote(610, DATE);
    assert_eq!(c.recent(), [Reaction { total_nanos: 510, work_nanos: 10, at: DATE }]);
}

#[test]
fn later_events_in_a_burst_do_not_move_the_start() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 100);
    c.event_arrived(300, 300);
    c.pass_began(600);
    c.wrote(610, DATE);
    assert_eq!(totals(&c), [510]);
}

#[test]
fn write_outside_a_scheduled_pass_is_ignored() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 100);
    c.wrote(200, DATE);
    assert!(c.recent().is_empty());
}

#[test]
fn write_without_an_event_is_ignored() {
    let mut c = ReactionClock::default();
    c.pass_began(100);
    c.wrote(110, DATE);
    assert!(c.recent().is_empty());
}

#[test]
fn a_pass_that_wrote_consumes_the_event() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 1100);
    c.pass_began(600);
    c.wrote(610, DATE);
    c.pass_ended(610);
    c.pass_began(1100);
    c.wrote(1110, DATE);
    assert_eq!(c.recent().len(), 1);
}

#[test]
fn a_burst_that_wrote_nothing_keeps_its_start_until_the_last_beat() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 1100);
    c.pass_began(600);
    c.pass_ended(600);
    c.pass_began(1100);
    c.wrote(1105, DATE);
    assert_eq!(totals(&c), [1005]);

    let mut quiet = ReactionClock::default();
    quiet.event_arrived(100, 1100);
    quiet.pass_began(600);
    quiet.pass_ended(600);
    quiet.pass_began(1100);
    quiet.pass_ended(1100);
    quiet.pass_began(1200);
    quiet.wrote(1205, DATE);
    assert!(quiet.recent().is_empty());
}

/// Swift 2026-10-07: the volume listener's echo of Cleat's own write must not start the next
/// reaction even though other beats are pending.
#[test]
fn the_echo_of_own_write_does_not_start_the_next_reaction() {
    let mut c = ReactionClock::default();
    c.event_arrived(100, 100);
    c.pass_began(110);
    c.wrote(120, DATE);
    c.pass_ended(120);
    c.event_arrived(130, 130);
    c.pass_began(140);
    c.pass_ended(150);
    c.event_arrived(1_900_000_000, 1_900_000_000);
    c.pass_began(1_900_000_010);
    c.wrote(1_900_000_020, DATE);
    assert_eq!(totals(&c), [20, 20]);
}

#[test]
fn ring_keeps_the_last_twenty_and_median_is_over_them() {
    let mut c = ReactionClock::default();
    for i in 1..=25u64 {
        let start = i * 1_000_000_000;
        c.event_arrived(start, start);
        c.pass_began(start);
        c.wrote(start + i * 1_000_000, DATE);
        c.pass_ended(start + i * 1_000_000);
    }
    assert_eq!(c.recent().len(), 20);
    let s = c.summary();
    assert_eq!(s.samples, 20);
    assert_eq!(s.last_reaction_ms, Some(25.0));
    // 6...25 ms: the middle two are 15 and 16.
    assert_eq!(s.median_reaction_ms, Some(15.5));

    let mut odd = ReactionClock::default();
    for ms in [9u64, 1, 5] {
        odd.event_arrived(0, 0);
        odd.pass_began(0);
        odd.wrote(ms * 1_000_000, DATE);
        odd.pass_ended(ms * 1_000_000);
    }
    assert_eq!(odd.summary().median_reaction_ms, Some(5.0));
}

#[test]
fn summary_with_no_reactions_says_zero_samples() {
    assert_eq!(ReactionClock::default().summary(), DaemonPerformance { samples: 0, ..Default::default() });
}

#[test]
fn format_keeps_digits_stable() {
    assert_eq!([millis_text(3.24), millis_text(512.4), millis_text(1023.0)], ["3.2 ms", "512 ms", "1.02 s"]);
}

fn swift_perf() -> DaemonPerformance {
    DaemonPerformance {
        samples: 2,
        last_reaction_ms: Some(3.5),
        last_work_ms: Some(0.4),
        median_reaction_ms: Some(3.0),
        last_reaction_at: Some("1970-01-01T00:16:40Z".into()),
    }
}

const OLD: &str = r#"{"pid": 9, "updatedAt": "2026-10-07T00:00:00Z", "configState": "ok", "microphone": "authorized", "rules": {}, "liveness": {}, "recentEvents": []}"#;

#[test]
fn status_round_trips_performance_and_reads_old_files_without() {
    let mut status: Status = serde_json::from_str(OLD).unwrap();
    assert_eq!(status.performance, None);
    assert_eq!(performance(OLD.as_bytes()), None);

    status.performance = Some(swift_perf());
    let path = std::env::temp_dir().join(format!("cleat-rs-perf-{}.json", std::process::id()));
    status.write(&path);
    let data = std::fs::read(&path).unwrap();
    let back = Status::read(&path);
    let _ = std::fs::remove_file(&path);
    assert_eq!(back, Some(status));
    assert_eq!(performance(&data), Some(swift_perf()));
    // Swift's key names, and nothing for an empty field.
    let empty = serde_json::to_value(DaemonPerformance::default()).unwrap();
    assert_eq!(empty, serde_json::json!({"samples": 0}));
    let full = serde_json::to_value(swift_perf()).unwrap();
    let keys: Vec<&String> = full.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["lastReactionAt", "lastReactionMs", "lastWorkMs", "medianReactionMs", "samples"]);
}

#[test]
fn engine_publishes_the_reaction_in_the_same_status_write() {
    let config = Config { input_volume: volumes(&[("Brio 100", 75.0)]), launch_at_login: false, ..Config::default() };
    let snap = DeviceSnapshot {
        devices: vec![brio()],
        default_input: Some(brio().id),
        input_volumes: [(brio().id, 0.5)].into_iter().collect(),
        ..Default::default()
    };
    // The baseline pass writes, but answers no event: not counted.
    let mut h = Harness::new(&config, snap, Opts::default());
    assert_eq!(h.status().performance.map(|p| p.samples), Some(0));

    h.set(|s| {
        s.input_volumes.insert(brio().id, 0.5);
    });
    h.fire(ListenerKind::Volume);
    h.drive(0);
    let id = brio().id;
    assert_eq!(h.writes(), [format!("volume:{id}:0.75"), format!("volume:{id}:0.75")]);
    assert_eq!(h.status().performance.map(|p| p.samples), Some(1));

    // The echo of that write finds nothing to do and adds nothing.
    h.set(|s| {
        s.input_volumes.insert(brio().id, 0.75);
    });
    h.fire(ListenerKind::Volume);
    h.drive(0);
    assert_eq!(h.status().performance.map(|p| p.samples), Some(1));
}

/// A beat later than the event: the total includes the wait, the work does not.
#[test]
fn settle_beats_count_toward_the_total_not_the_work() {
    let config = Config { balance: Some(0.5), launch_at_login: false, ..Config::default() };
    let speakers = mac_studio_speakers();
    let snap = DeviceSnapshot {
        devices: vec![speakers.clone()],
        default_output: Some(speakers.id),
        output_balance: Some(0.5),
        ..Default::default()
    };
    let mut h = Harness::new(&config, snap, Opts::default());
    h.set(|s| s.output_balance = Some(0.2));
    h.fire(ListenerKind::Balance);
    h.drive(1000);
    let p = h.status().performance.unwrap();
    assert_eq!(p.samples, 1);
    assert_eq!(p.last_reaction_ms, Some(cleat_rs::engine::BALANCE_SETTLE_MS as f64));
    assert_eq!(p.last_work_ms, Some(0.0));
}

fn running(p: Option<DaemonPerformance>) -> DaemonVitals {
    DaemonVitals { performance: p, ..DaemonVitals::with_state(VitalsState::Running) }
}

#[test]
fn vitals_band_cell_has_swifts_three_branches() {
    let old = running(None);
    assert_eq!((reaction_value(&old), reaction_note(&old)), ("—".into(), "還沒有拉回紀錄".into()));
    assert_eq!(reaction_help(&old), "這個版本的 Cleat 還不會量拉回速度");

    let fresh = running(Some(DaemonPerformance::default()));
    assert_eq!(reaction_value(&fresh), "—");
    assert_eq!(reaction_help(&fresh), "還沒有拉回紀錄");

    let some = running(Some(swift_perf()));
    assert_eq!((reaction_value(&some), reaction_note(&some)), ("3.0 ms".into(), "最近 2 次的中位數".into()));
    assert_eq!(
        reaction_help(&some),
        "其他程式或系統改掉你的設定後，Cleat 改回來要多久。最近一次共 3.5 ms，其中 Cleat 自己處理 0.4 ms，其餘是刻意等裝置穩定"
    );

    let stopped = DaemonVitals { performance: Some(swift_perf()), ..DaemonVitals::with_state(VitalsState::NotRunning) };
    assert_eq!(reaction_value(&stopped), "—");
    assert_eq!(reaction_help(&stopped), "Cleat 沒在執行");
}

#[test]
fn cli_reaction_line() {
    let line = |p: Option<DaemonPerformance>, alive: bool| {
        let mut s: Status = serde_json::from_str(OLD).unwrap();
        s.performance = p;
        let (text, _) = render_status(Some(&s), alive, "x", "s", "c");
        text.lines().find(|l| l.starts_with("reaction:")).map(String::from)
    };
    assert_eq!(line(None, true).as_deref(), Some("reaction:    -"));
    assert_eq!(line(Some(DaemonPerformance::default()), true).as_deref(), Some("reaction:    no write-back yet"));
    assert_eq!(
        line(Some(swift_perf()), true).as_deref(),
        Some("reaction:    last 3.5 ms (work 0.4 ms), median 3.0 ms over 2")
    );
    assert_eq!(line(Some(swift_perf()), false), None);
}
