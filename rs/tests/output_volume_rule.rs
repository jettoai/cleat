//! The output volume hold's decision and its memory, driven by explicit times (Swift
//! `OutputVolumeHoldRuleTests.swift`, 16 tests).

mod common;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cleat_rs::model::{Action, AudioDevice};
use cleat_rs::outvol::ledger::{Judgement, Ledger};
use cleat_rs::outvol::writer_log::VolumeWrite;
use cleat_rs::rules::output_volume_hold::{actions, decide, Foreign, Input, Prior, Verdict};
use common::*;

fn t0() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs_f64(1_791_339_595.713)
}
fn at(offset: f64) -> SystemTime {
    if offset >= 0.0 { t0() + Duration::from_secs_f64(offset) } else { t0() - Duration::from_secs_f64(-offset) }
}
fn device() -> AudioDevice {
    air_pods()
}
fn w(writer: &str, offset: f64, from: f32, to: f32, pid: i32, control: i64) -> VolumeWrite {
    VolumeWrite { at: at(offset), pid, writer: writer.into(), control, from, to }
}
fn prl(offset: f64, from: f32, to: f32) -> VolumeWrite {
    w("prl_vm_app", offset, from, to, 8484, 262)
}
fn foreign(restore: f32, results: &[f32], overruled: bool) -> Foreign {
    Foreign { restore, writer: "prl_vm_app".into(), results: results.to_vec(), overruled, prior_writer: None }
}
fn kept(from: f32, to: f32, writer: &str, note: Option<&str>) -> Verdict {
    Verdict::Kept { from, to, writer: writer.into(), note: note.map(String::from) }
}
fn revert(from: f32, restore: f32, prior: Option<Prior>) -> Verdict {
    Verdict::Revert { from, restore, writer: "prl_vm_app".into(), prior }
}
fn input(current: &[f32], held: Option<f32>, foreign: Option<Foreign>, last: Option<&str>) -> Input {
    Input { current: current.to_vec(), held, foreign, last_writer: last.map(String::from), paused: false }
}

/// A ledger that has already adopted `level` for the device.
fn ledger(level: f32) -> Ledger {
    let mut l = Ledger::default();
    l.observe(&[level, level], &device().uid, at(-10.0));
    l.judge(&device(), &[level, level], at(-10.0), false);
    l
}
fn output(l: &mut Ledger, offset: f64, a: f32, b: f32) {
    l.observe(&[a, b], &device().uid, at(offset));
}
fn record(l: &mut Ledger, write: VolumeWrite, listed: bool, now: f64) -> Option<SystemTime> {
    l.record(write, listed, 1, Some(&device().uid), at(now))
}
fn judge(l: &mut Ledger, current: &[f32], now: f64) -> Verdict {
    l.judge(&device(), current, at(now), false).verdict
}

/// Records a listed write, lands it on both channels and judges at its deadline.
fn parallels(l: &mut Ledger, offset: f64, from: f32, to: f32) -> Judgement {
    let line = prl(offset, from, to);
    let deadline = l.record(line.clone(), true, 1, Some(&device().uid), line.at);
    assert!(deadline.is_some());
    output(l, offset - 0.01, from, from);
    output(l, offset, to, to);
    l.volume_changed(line.at);
    l.judge(&device(), &[to, to], deadline.unwrap_or(line.at), false)
}

#[test]
fn listed_write_still_in_place_is_reverted() {
    let v = decide(&input(&[0.427210, 0.427210], Some(0.488189), Some(foreign(0.488189, &[0.427210], false)), None));
    assert_eq!(v, revert(0.427210, 0.488189, None));
    assert_eq!(
        actions(&v, &device()),
        vec![Action::SetOutputVolume(device().id, 0.488189, "AirPods Max 43% -> 49% (reverted prl_vm_app)".into())]
    );
}

#[test]
fn overruled_or_moved_on_writes_are_kept() {
    let overruled = decide(&input(&[0.5, 0.5], Some(0.625), Some(foreign(0.625, &[0.53], true)), Some("ControlCenter")));
    assert_eq!(overruled, kept(0.625, 0.5, "ControlCenter", None));
    let crown = decide(&input(&[0.6, 0.6], Some(0.625), Some(foreign(0.625, &[0.53], false)), None));
    assert_eq!(crown, kept(0.625, 0.6, "prl_vm_app", None));
    assert!(actions(&crown, &device()).is_empty());
}

#[test]
fn adopt_and_nothing() {
    assert_eq!(decide(&input(&[0.4, 0.6], None, None, None)), Verdict::Adopt(0.5));
    assert_eq!(decide(&input(&[0.3, 0.3], Some(0.5), None, Some("cleat"))), Verdict::Adopt(0.3));
    assert_eq!(decide(&input(&[0.5, 0.5], Some(0.5), None, Some("Jetto"))), Verdict::None);
    assert_eq!(decide(&input(&[], Some(0.5), None, None)), Verdict::None);
}

#[test]
fn channels_written_apart_are_one_revert() {
    let mut l = ledger(0.5);
    let left = prl(0.0, 0.5, 0.43);
    let right = w("prl_vm_app", 0.249, 0.5, 0.43, 8484, 263);
    assert_eq!(record(&mut l, left, true, 0.0), Some(at(0.3)));
    output(&mut l, 0.0, 0.43, 0.5);
    assert_eq!(record(&mut l, right, true, 0.249), None);
    output(&mut l, 0.249, 0.43, 0.43);
    l.volume_changed(at(0.249));
    assert_eq!(judge(&mut l, &[0.43, 0.5], 0.25), Verdict::None);
    assert_eq!(judge(&mut l, &[0.43, 0.43], 0.3), revert(0.43, 0.5, None));
    assert_eq!(l.revert_count(), 1);
    assert_eq!(l.held(&device().uid), Some(0.5));
}

#[test]
fn echo_joins_the_write_but_a_later_change_overrules_it() {
    let mut echoed = ledger(0.625);
    record(&mut echoed, prl(0.0, 0.625, 0.531160), true, 0.0);
    record(&mut echoed, w("ControlCenter", 0.016, 0.531160, 0.5625, 531, 262), false, 0.016);
    output(&mut echoed, 0.0, 0.531160, 0.531160);
    output(&mut echoed, 0.016, 0.5625, 0.5625);
    assert_eq!(judge(&mut echoed, &[0.5625, 0.5625], 0.3), revert(0.5625, 0.625, None));

    let mut overruled = ledger(0.625);
    record(&mut overruled, prl(0.0, 0.625, 0.531160), true, 0.0);
    record(&mut overruled, w("ControlCenter", 0.2, 0.531160, 0.5, 531, 262), false, 0.2);
    output(&mut overruled, 0.0, 0.531160, 0.531160);
    output(&mut overruled, 0.2, 0.5, 0.5);
    assert_eq!(judge(&mut overruled, &[0.5, 0.5], 0.5), kept(0.625, 0.5, "ControlCenter", None));
}

#[test]
fn only_control_center_counts_as_echo() {
    let mut l = ledger(0.5);
    record(&mut l, prl(0.0, 0.5, 0.43), true, 0.0);
    record(&mut l, w("Jetto", 0.05, 0.43, 0.1, 72504, 262), false, 0.05);
    output(&mut l, 0.0, 0.43, 0.43);
    output(&mut l, 0.05, 0.1, 0.1);
    assert_eq!(judge(&mut l, &[0.1, 0.1], 0.3), kept(0.5, 0.1, "Jetto", None));
    assert_eq!(l.held(&device().uid), Some(0.1));
}

#[test]
fn stale_line_opens_no_streak() {
    let mut l = ledger(0.5);
    assert_eq!(record(&mut l, prl(0.0, 0.5, 0.43), true, 1.5), None);
    assert!(!l.has_streak());
}

#[test]
fn tug_of_war_pauses_and_resumes() {
    let mut l = ledger(0.5);
    for round in 0..3 {
        let j = parallels(&mut l, round as f64 * 10.0, 0.5, 0.43);
        assert_eq!(j.verdict, revert(0.43, 0.5, None));
        assert!(!j.started_pause);
    }
    let fourth = parallels(&mut l, 30.0, 0.5, 0.43);
    assert_eq!(fourth.verdict, kept(0.5, 0.43, "prl_vm_app", Some("paused")));
    assert!(fourth.started_pause);
    let until = l.paused_until.unwrap().duration_since(t0()).unwrap().as_secs_f64();
    assert!((until - 630.3).abs() < 0.001, "{until}");
    assert_eq!(l.paused_by.as_deref(), Some("prl_vm_app"));
    let during = parallels(&mut l, 40.0, 0.43, 0.36);
    assert_eq!(during.verdict, kept(0.43, 0.36, "prl_vm_app", Some("paused")));
    assert_eq!(l.held(&device().uid), Some(0.36));
    let after = parallels(&mut l, 700.0, 0.36, 0.3);
    assert_eq!(after.verdict, revert(0.3, 0.36, None));
    assert!(l.paused_until.is_none());
}

#[test]
fn ten_bluetooth_changes_without_a_writer_line_is_blind_until_a_line_parses() {
    let mut l = ledger(0.5);
    for step in 1..=10 {
        let level = 0.5 - step as f32 * 0.01;
        l.volume_changed(at(step as f64));
        l.judge(&device(), &[level, level], at(step as f64 + 0.3), false);
        assert_eq!(l.blind().is_some(), step >= 10, "step {step}");
    }
    record(&mut l, w("Jetto", 20.0, 0.4, 0.2, 72504, 262), false, 20.0);
    assert!(l.blind().is_none());

    let mut speakers = Ledger::default();
    for step in 0..=12 {
        let level = 0.5 - step as f32 * 0.01;
        speakers.judge(&display_speakers(), &[level], at(step as f64), false);
    }
    assert!(speakers.blind().is_none());
}

#[test]
fn streak_for_another_device_is_dropped() {
    let mut l = ledger(0.5);
    l.record(prl(0.0, 0.5, 0.43), true, 1, Some("other-UID"), at(0.0));
    output(&mut l, 0.0, 0.43, 0.43);
    assert_eq!(judge(&mut l, &[0.43, 0.43], 0.3), kept(0.5, 0.43, "unknown", None));
    assert!(!l.has_streak());
}

#[test]
fn switch_to_another_device_is_not_an_output_change() {
    let mut l = ledger(0.8);
    let speakers = mac_speakers();
    l.observe(&[0.4, 0.4], &speakers.uid, at(0.0));
    assert_eq!(l.judge(&speakers, &[0.4, 0.4], at(0.0), false).verdict, Verdict::Adopt(0.4));
    l.record(w("prl_vm_app", 0.05, 0.8, 0.4, 8484, 261), true, 1, Some(&speakers.uid), at(0.055));
    assert_eq!(l.judge(&speakers, &[0.4, 0.4], at(0.35), false).verdict, Verdict::None);
    assert_eq!(l.held(&speakers.uid), Some(0.4));
}

/// A listed line, as a line and as the output change it caused, in either arrival order.
fn listed_write(l: &mut Ledger, offset: f64, from: f32, to: f32, listener_first: bool, lag: f64) {
    let line = prl(offset, from, to);
    if listener_first {
        output(l, offset + lag, to, to);
    }
    record(l, line, true, offset + 0.005);
    if !listener_first {
        output(l, offset + lag, to, to);
    }
    l.volume_changed(at(offset + lag));
}

#[test]
fn mixed_controls_restore_the_outputs_own_value() {
    let mut l = ledger(0.5);
    record(&mut l, w("prl_vm_app", 0.0, 1.0, 0.4, 8484, 261), true, 0.005);
    listed_write(&mut l, 0.1, 0.5, 0.4, false, 0.0);
    assert_eq!(judge(&mut l, &[0.4, 0.4], 0.3), revert(0.4, 0.5, None));
}

#[test]
fn user_change_not_yet_judged_is_restored() {
    let mut l = ledger(0.5);
    output(&mut l, 0.0, 0.6, 0.6);
    l.volume_changed(at(0.0));
    listed_write(&mut l, 0.1, 0.6, 0.5, false, 0.0);
    assert_eq!(judge(&mut l, &[0.5, 0.5], 0.3), Verdict::None);
    assert_eq!(
        judge(&mut l, &[0.5, 0.5], 0.4),
        revert(0.5, 0.6, Some(Prior { from: 0.5, to: 0.6, writer: "unknown".into() }))
    );
    assert_eq!(l.held(&device().uid), Some(0.6));
}

#[test]
fn listed_write_that_did_not_move_the_output_is_ignored() {
    let mut l = ledger(0.5);
    record(&mut l, w("prl_vm_app", 0.0, 1.0, 0.5, 8484, 261), true, 0.005);
    output(&mut l, 0.01, 0.5, 0.5);
    assert_eq!(judge(&mut l, &[0.5, 0.5], 0.3), Verdict::None);
    assert_eq!(l.held(&device().uid), Some(0.5));
}

#[test]
fn line_and_listener_may_arrive_in_either_order() {
    for first in [false, true] {
        let mut l = ledger(0.5);
        listed_write(&mut l, 0.0, 0.5, 0.43, first, 0.02);
        assert_eq!(judge(&mut l, &[0.43, 0.43], 0.3), revert(0.43, 0.5, None), "listener first {first}");
    }
}

#[test]
fn output_change_too_far_from_the_line_is_not_its_write() {
    let mut l = ledger(0.5);
    listed_write(&mut l, 0.0, 0.5, 0.43, false, 0.2);
    assert_eq!(judge(&mut l, &[0.43, 0.43], 0.5), kept(0.5, 0.43, "unknown", None));
}
