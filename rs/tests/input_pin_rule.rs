mod common;

use std::collections::HashMap;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot, Liveness};
use cleat_rs::rules::input_pin::reconcile;
use common::*;

fn snapshot(devices: Vec<AudioDevice>, current: Option<&AudioDevice>, liveness: &[(&AudioDevice, Liveness)]) -> DeviceSnapshot {
    DeviceSnapshot {
        devices,
        default_input: current.map(|d| d.id),
        liveness: liveness.iter().map(|(d, l)| (d.uid.clone(), *l)).collect::<HashMap<_, _>>(),
        ..Default::default()
    }
}

#[test]
fn current_is_target_does_nothing() {
    let snap = snapshot(vec![wireless(), brio()], Some(&wireless()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn higher_priority_device_takes_over() {
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(wireless().id, "Brio 100 -> Wireless microphone (higher priority present)".into())]
    );
}

#[test]
fn silent_device_falls_back_to_next_in_list() {
    let snap = snapshot(vec![wireless(), brio()], Some(&wireless()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(brio().id, "Wireless microphone -> Brio 100 (Wireless microphone silent)".into())]
    );
}

#[test]
fn blocked_device_is_replaced() {
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(brio().id, "AirPods Max -> Brio 100 (blocked)".into())]
    );
}

#[test]
fn virtual_conferencing_device_is_left_alone() {
    let snap = snapshot(vec![zoom(), wireless(), brio()], Some(&zoom()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn hand_picked_device_is_left_alone() {
    let snap = snapshot(vec![maono(), wireless(), brio()], Some(&maono()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn silent_top_device_but_already_on_fallback() {
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn no_candidate_leaves_blocked_device_in_place() {
    let snap = snapshot(vec![air_pods()], Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn measuring_device_is_not_a_switch_target() {
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[(&wireless(), Liveness::Measuring)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn measuring_incumbent_is_not_evicted() {
    let snap = snapshot(vec![wireless(), brio()], Some(&wireless()), &[(&wireless(), Liveness::Measuring)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn empty_priority_list_disables_the_rule() {
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &Config::default()), vec![]);
}

#[test]
fn untracked_liveness_counts_as_present() {
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(wireless().id, "Brio 100 -> Wireless microphone (higher priority present)".into())]
    );
}

#[test]
fn output_only_device_is_not_an_input_candidate() {
    let config =
        Config { input: s(&["Studio Display Speakers", "Brio 100"]), blocked_input: s(&["AirPods Max"]), ..Config::default() };
    let snap = snapshot(vec![display_speakers(), air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(brio().id, "AirPods Max -> Brio 100 (blocked)".into())]
    );
}

#[test]
fn uid_entry_matches_the_device() {
    let config = Config {
        input: vec![wireless().uid, "Brio 100".into()],
        blocked_input: s(&["AirPods Max"]),
        ..Config::default()
    };
    let snap = snapshot(vec![wireless(), brio()], Some(&brio()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(wireless().id, "Brio 100 -> Wireless microphone (higher priority present)".into())]
    );
}

#[test]
fn no_default_input_does_nothing() {
    let snap = snapshot(vec![wireless(), brio()], None, &[]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}
