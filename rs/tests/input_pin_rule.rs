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

// A device on both the priority list and the blocked list is never a target: blocked outranks listed.

fn overlap(input: &[&str], blocked: &[&str]) -> Config {
    Config { input: s(input), blocked_input: s(blocked), ..Config::default() }
}

#[test]
fn albert_config_does_not_fall_back_to_a_blocked_listed_device() {
    let config = overlap(&["Wireless microphone", "Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![wireless(), air_pods()], Some(&wireless()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn blocked_device_listed_in_the_middle_is_skipped() {
    let config = overlap(&["Wireless microphone", "AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap =
        snapshot(vec![wireless(), air_pods(), brio()], Some(&wireless()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(brio().id, "Wireless microphone -> Brio 100 (Wireless microphone silent)".into())]
    );
}

#[test]
fn blocked_device_listed_first_does_not_take_over() {
    let config = overlap(&["AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), brio()], Some(&brio()), &[]);
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn uid_block_outranks_a_name_entry() {
    let config = overlap(&["Brio 100", "Wireless microphone"], &[&brio().uid]);
    let snap = snapshot(vec![brio(), wireless()], Some(&wireless()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn unblocked_current_input_is_not_evicted_without_a_candidate() {
    let snap = snapshot(
        vec![wireless(), brio(), maono()],
        Some(&brio()),
        &[(&wireless(), Liveness::Silent), (&brio(), Liveness::Silent)],
    );
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

// A device cleat placed there gives way to a returning listed device. A blocked current input is
// rules::blocked's case (tests/blocked_rule.rs); this rule leaves it alone (B-1287).

#[test]
fn a_blocked_current_input_is_not_this_rules_case() {
    let snap = snapshot(vec![air_pods(), wireless()], Some(&air_pods()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn placed_device_gives_way_to_a_returning_listed_device() {
    let mut snap = snapshot(vec![mac_mic(), wireless()], Some(&mac_mic()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
    snap.placed_input = Some(mac_mic().uid);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(
            wireless().id,
            "MacBook Pro Microphone -> Wireless microphone (listed device back)".into()
        )]
    );
}

