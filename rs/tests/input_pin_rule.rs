mod common;

use std::collections::HashMap;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot, Liveness, TRANSPORT_BUILT_IN};
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
fn blocked_listed_current_device_is_replaced_by_the_next_candidate() {
    let config = overlap(&["AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(brio().id, "AirPods Max -> Brio 100 (blocked)".into())]
    );
}

#[test]
fn blocked_listed_current_device_moves_to_a_silent_listed_device() {
    let config = overlap(&["Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), brio()], Some(&air_pods()), &[(&brio(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(brio().id, "AirPods Max -> Brio 100 (blocked)".into())]
    );
}

#[test]
fn blocked_listed_current_device_stays_without_another_candidate() {
    let config = overlap(&["AirPods Max", "Brio 100"], &["AirPods Max"]);
    let snap = snapshot(vec![air_pods()], Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn uid_block_outranks_a_name_entry() {
    let config = overlap(&["Brio 100", "Wireless microphone"], &[&brio().uid]);
    let snap = snapshot(vec![brio(), wireless()], Some(&wireless()), &[(&wireless(), Liveness::Live)]);
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn uid_block_skips_to_the_next_candidate_from_outside_the_list() {
    let config = overlap(&["Brio 100", "Wireless microphone"], &[&brio().uid, "AirPods Max"]);
    let snap = snapshot(vec![brio(), wireless(), air_pods()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(wireless().id, "AirPods Max -> Wireless microphone (blocked)".into())]
    );
}

// A blocked current input with no live candidate is still evicted.

#[test]
fn albert_config_evicts_air_pods_to_the_built_in_microphone_before_the_silent_wireless_one() {
    let config = overlap(&["Wireless microphone", "Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![wireless(), air_pods(), mac_mic()], Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
}

#[test]
fn without_a_built_in_microphone_air_pods_go_to_the_silent_wireless_microphone() {
    let config = overlap(&["Wireless microphone", "Brio 100", "AirPods Max"], &["AirPods Max"]);
    let snap = snapshot(vec![wireless(), air_pods()], Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(wireless().id, "AirPods Max -> Wireless microphone (blocked)".into())]
    );
}

#[test]
fn a_measuring_listed_device_also_comes_after_the_built_in_microphone() {
    let snap = snapshot(vec![wireless(), air_pods(), mac_mic()], Some(&air_pods()), &[(&wireless(), Liveness::Measuring)]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
}

#[test]
fn blocked_current_input_falls_back_to_the_built_in_microphone_not_an_unlisted_one() {
    let snap = snapshot(vec![air_pods(), maono(), mac_mic()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &pinned_input()),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
    let snap = snapshot(vec![air_pods(), maono()], Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

#[test]
fn built_in_fallback_skips_blocked_and_output_only_devices() {
    let config = overlap(&["Wireless microphone"], &["AirPods Max", "Brio 100", "MacBook Pro Microphone"]);
    let other_mic = AudioDevice::with_transport(84, "Aaa Microphone", "Aaa-UID", true, false, TRANSPORT_BUILT_IN);
    let snap = snapshot(vec![air_pods(), brio(), mac_speakers(), mac_mic(), other_mic.clone()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(other_mic.id, "AirPods Max -> Aaa Microphone (blocked)".into())]
    );
    let snap = snapshot(vec![air_pods(), brio(), mac_speakers(), mac_mic()], Some(&air_pods()), &[]);
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

// B-1283 return: outside the list only the built-in microphone is a fallback, an empty list still
// evicts, and a device cleat placed there gives way to a returning listed device.

fn albert_extras() -> Vec<AudioDevice> {
    vec![air_pods(), zoom(), black_hole(), iphone_mic()]
}

/// Albert's replay (a1): AirPods Max current, wireless microphone and Brio absent.
#[test]
fn blocked_input_falls_back_to_the_built_in_microphone_only() {
    let mut devices = albert_extras();
    devices.push(mac_mic());
    let snap = snapshot(devices, Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &albert_input()),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
}

#[test]
fn blocked_input_without_a_built_in_microphone_stays() {
    let snap = snapshot(albert_extras(), Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &albert_input()), vec![]);
}

#[test]
fn only_virtual_inputs_are_never_a_fallback() {
    let snap = snapshot(vec![air_pods(), zoom()], Some(&air_pods()), &[]);
    assert_eq!(reconcile(&snap, &pinned_input()), vec![]);
}

/// Albert's replay (a2): the wireless microphone is there but silent; AirPods Max goes to the
/// built-in microphone, and back to the wireless one once it has signal (engine_eviction).
#[test]
fn albert_replay_silent_wireless_microphone_yields_to_the_built_in() {
    let mut devices = albert_extras();
    devices.extend([mac_mic(), wireless()]);
    let snap = snapshot(devices, Some(&air_pods()), &[(&wireless(), Liveness::Silent)]);
    assert_eq!(
        reconcile(&snap, &albert_input()),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
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

#[test]
fn blocked_current_input_is_evicted_with_an_empty_list() {
    let config = Config { blocked_input: s(&["AirPods Max"]), ..Config::default() };
    let snap = snapshot(vec![air_pods(), mac_mic()], Some(&air_pods()), &[]);
    assert_eq!(
        reconcile(&snap, &config),
        vec![Action::SetDefaultInput(mac_mic().id, "AirPods Max -> MacBook Pro Microphone (blocked)".into())]
    );
}
