mod common;

use std::collections::HashSet;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, BluetoothHeadset, DeviceSnapshot, TRANSPORT_BLUETOOTH};
use cleat_rs::rules::reclaim::reconcile;
use common::*;

const AIR_PODS_ADDRESS: &str = "70:F9:4A:B6:0C:C9";

fn headset(name: &str, address: &str, connected: bool) -> BluetoothHeadset {
    BluetoothHeadset { name: name.into(), address: address.into(), is_connected: connected }
}
fn connected_air_pods() -> BluetoothHeadset {
    headset("AirPods Max", AIR_PODS_ADDRESS, true)
}
fn connected_air_pods_pro() -> BluetoothHeadset {
    headset("AirPods Pro", "7C:F3:4D:68:72:76", true)
}
fn mouse() -> BluetoothHeadset {
    headset("MX Master 3S", "D0:70:FB:5A:99:FE", true)
}

fn config() -> Config {
    Config { reclaim: s(&["AirPods Max"]), ..Config::default() }
}

fn playing(devices: Vec<AudioDevice>) -> DeviceSnapshot {
    let default_output = devices.first().map(|d| d.id);
    DeviceSnapshot { devices, default_output, output_running: true, ..Default::default() }
}

fn speakers_playing() -> DeviceSnapshot {
    playing(vec![mac_studio_speakers()])
}

/// The reason written out literally, so the assertion does not borrow the implementation's format.
fn request(h: &BluetoothHeadset) -> Action {
    Action::RequestRoute {
        name: h.name.clone(),
        address: h.address.clone(),
        reason: format!("cleat: {} connected but not an audio device, Mac is playing", h.name),
    }
}

fn none() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn all_four_conditions_ask_for_the_headset() {
    assert_eq!(
        reconcile(&speakers_playing(), &[connected_air_pods()], &config(), &none()),
        vec![request(&connected_air_pods())]
    );
}

#[test]
fn unlisted_headset_is_left_alone() {
    assert_eq!(reconcile(&speakers_playing(), &[connected_air_pods_pro(), mouse()], &config(), &none()), vec![]);
}

#[test]
fn disconnected_headset_is_not_asked_for() {
    let away = headset("AirPods Max", AIR_PODS_ADDRESS, false);
    assert_eq!(reconcile(&speakers_playing(), &[away], &config(), &none()), vec![]);
}

#[test]
fn headset_here_but_not_the_output_is_asked_for() {
    let snap = playing(vec![mac_studio_speakers(), air_pods()]);
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &config(), &none()), vec![request(&connected_air_pods())]);
}

#[test]
fn headset_that_is_the_output_is_not_asked_for() {
    let snap = playing(vec![air_pods(), mac_studio_speakers()]);
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &config(), &none()), vec![]);
}

#[test]
fn headset_present_as_input_only_is_still_asked_for() {
    let input_side = AudioDevice::with_transport(33, "AirPods Max", "AirPodsMax-Input-UID", true, false, TRANSPORT_BLUETOOTH);
    let snap = playing(vec![mac_studio_speakers(), input_side]);
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &config(), &none()), vec![request(&connected_air_pods())]);
}

#[test]
fn idle_mac_does_not_ask_for_anything() {
    let mut snap = speakers_playing();
    snap.output_running = false;
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &config(), &none()), vec![]);
}

#[test]
fn empty_config_turns_the_rule_off() {
    assert_eq!(reconcile(&speakers_playing(), &[connected_air_pods()], &Config::default(), &none()), vec![]);
}

#[test]
fn two_listed_headsets_ask_for_one_of_them_by_name() {
    let mut config = config();
    config.reclaim = s(&["AirPods Max", "AirPods Pro"]);
    assert_eq!(
        reconcile(&speakers_playing(), &[connected_air_pods_pro(), connected_air_pods()], &config, &none()),
        vec![request(&connected_air_pods())]
    );
}

#[test]
fn blocked_headset_steps_aside_for_the_next_one() {
    let mut config = config();
    config.reclaim = s(&["AirPods Max", "AirPods Pro"]);
    let both = [connected_air_pods_pro(), connected_air_pods()];

    let one: HashSet<String> = [connected_air_pods().address].into_iter().collect();
    assert_eq!(reconcile(&speakers_playing(), &both, &config, &one), vec![request(&connected_air_pods_pro())]);

    let all: HashSet<String> = [connected_air_pods().address, connected_air_pods_pro().address].into_iter().collect();
    assert_eq!(reconcile(&speakers_playing(), &both, &config, &all), vec![]);
}

#[test]
fn headset_is_named_by_address_in_either_form() {
    assert!(connected_air_pods().is_listed(&s(&[AIR_PODS_ADDRESS])));
    assert!(connected_air_pods().is_listed(&s(&["70-f9-4a-b6-0c-c9"])));
    assert!(!connected_air_pods().is_listed(&s(&["70:F9:4A:B6:0C:CA"])));
}

#[test]
fn name_matching_is_case_sensitive_and_address_matching_is_not() {
    assert!(!connected_air_pods().is_listed(&s(&["airpods max"])));
    assert!(connected_air_pods().is_listed(&s(&["70:f9:4a:b6:0c:c9"])));
}

#[test]
fn canonical_address_normalises_what_the_bluetooth_pane_shows() {
    assert_eq!(BluetoothHeadset::canonical_address("70-f9-4a-b6-0c-c9"), AIR_PODS_ADDRESS);
    assert_eq!(BluetoothHeadset::canonical_address(AIR_PODS_ADDRESS), AIR_PODS_ADDRESS);
}

// A headset on blockedOutput is "never this one" for the output: it is not asked back either.

fn blocked_headset_config(blocked: &[&str]) -> Config {
    Config {
        output: s(&["MacBook Pro Speakers"]),
        blocked_output: s(blocked),
        reclaim: s(&["AirPods Max"]),
        ..Config::default()
    }
}

#[test]
fn headset_blocked_by_name_is_not_asked_back() {
    let snap = playing(vec![mac_speakers()]);
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &blocked_headset_config(&["AirPods Max"]), &none()), vec![]);
}

#[test]
fn headset_blocked_by_address_is_not_asked_back() {
    let snap = playing(vec![mac_speakers()]);
    let config = blocked_headset_config(&["70-f9-4a-b6-0c-c9"]);
    assert_eq!(reconcile(&snap, &[connected_air_pods()], &config, &none()), vec![]);
}

#[test]
fn blocking_one_headset_still_asks_for_the_other() {
    let snap = playing(vec![mac_speakers()]);
    let mut config = blocked_headset_config(&["AirPods Max"]);
    config.reclaim = s(&["AirPods Max", "AirPods Pro"]);
    assert_eq!(
        reconcile(&snap, &[connected_air_pods(), connected_air_pods_pro()], &config, &none()),
        vec![request(&connected_air_pods_pro())]
    );
}
