//! EngineTests "Balance settle" (Swift EngineTests.swift :728-:786, fa89ada). `drive(ms)` stands in
//! for Swift's `waitOnQueue`, on a fake clock.

mod common;

use cleat_rs::audio::ListenerKind;
use cleat_rs::config::Config;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{Harness, Opts};
use common::*;

fn start_balance_lock() -> Harness {
    let config = Config { balance: Some(0.5), launch_at_login: false, ..Config::default() };
    let speakers = display_speakers();
    let snap = DeviceSnapshot {
        devices: vec![speakers.clone()],
        default_output: Some(speakers.id),
        output_balance: Some(0.5),
        ..Default::default()
    };
    let h = Harness::new(&config, snap, Opts::default());
    assert!(h.writes().is_empty());
    h
}

fn balance_lines(h: &Harness) -> Vec<String> {
    h.log_lines().into_iter().filter(|l| l.contains("balance:")).collect()
}

/// Parallels writes left and right 238ms apart; judged between the two the balance looks off.
#[test]
fn volume_written_one_channel_at_a_time_is_not_taken_for_a_balance_shift() {
    let mut h = start_balance_lock();
    h.set(|s| s.output_balance = Some(0.36));
    h.fire(ListenerKind::Balance);
    h.drive(240);
    h.set(|s| s.output_balance = Some(0.5));
    h.fire(ListenerKind::Balance);
    h.drive(760);

    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(balance_lines(&h).is_empty());
}

/// A zero-delay pass landing between the two channel writes must not correct the half-written
/// balance either.
#[test]
fn unrelated_beat_between_the_channel_writes_leaves_the_balance_alone() {
    let mut h = start_balance_lock();
    h.set(|s| s.output_balance = Some(0.36));
    h.fire(ListenerKind::Balance);
    h.drive(100);
    h.engine.schedule_reconcile(0);
    h.drive(140);
    h.set(|s| s.output_balance = Some(0.5));
    h.fire(ListenerKind::Balance);
    h.drive(760);

    assert!(h.writes().is_empty(), "{:?}", h.writes());
    assert!(balance_lines(&h).is_empty());
}

#[test]
fn balance_that_stays_off_is_still_corrected() {
    let mut h = start_balance_lock();
    h.set(|s| s.output_balance = Some(0.36));
    h.fire(ListenerKind::Balance);
    h.drive(1000);

    assert_eq!(h.writes(), vec![format!("balance:{}:0.50", display_speakers().id)]);
}
