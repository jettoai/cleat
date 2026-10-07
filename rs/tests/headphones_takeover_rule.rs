mod common;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot, TRANSPORT_BLUETOOTH, TRANSPORT_BLUETOOTH_LE};
use cleat_rs::rules::headphones_takeover::{has_eligible_arrival, reconcile};
use common::*;

fn config() -> Config {
    Config { output: s(&["Studio Display Speakers"]), headphones_take_over: true, ..Config::default() }
}

fn air_pods_pro() -> AudioDevice {
    AudioDevice::with_transport(31, "AirPods Pro", "AirPodsPro-UID", true, true, TRANSPORT_BLUETOOTH)
}

fn bluetooth_mic() -> AudioDevice {
    AudioDevice::with_transport(32, "Bluetooth Mic", "BluetoothMic-UID", true, false, TRANSPORT_BLUETOOTH_LE)
}

fn snapshot(devices: Vec<AudioDevice>, current: Option<&AudioDevice>, arrived: &[AudioDevice]) -> DeviceSnapshot {
    DeviceSnapshot {
        devices,
        default_output: current.map(|d| d.id),
        arrived: arrived.iter().map(|d| d.uid.clone()).collect(),
        ..Default::default()
    }
}

#[test]
fn arrived_bluetooth_output_takes_over() {
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&mac_studio_speakers()), &[air_pods()]);
    assert_eq!(
        reconcile(&snap, &config()),
        vec![Action::SetDefaultOutput(air_pods().id, "Mac Studio的揚聲器 -> AirPods Max (headphones connected)".into())]
    );
}

#[test]
fn arrived_wired_output_is_not_taken_over() {
    let snap =
        snapshot(vec![mac_studio_speakers(), wired_headphones()], Some(&mac_studio_speakers()), &[wired_headphones()]);
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn arrived_bluetooth_input_only_device_is_not_an_output() {
    let snap = snapshot(vec![mac_studio_speakers(), bluetooth_mic()], Some(&mac_studio_speakers()), &[bluetooth_mic()]);
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn arrived_headphones_already_the_default_output_do_nothing() {
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&air_pods()), &[air_pods()]);
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn arrived_bluetooth_output_on_the_blocked_list_is_not_taken_over() {
    let mut config = config();
    config.blocked_output = s(&["AirPods Max"]);
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&mac_studio_speakers()), &[air_pods()]);
    assert_eq!(reconcile(&snap, &config), vec![]);
    assert!(!has_eligible_arrival(&snap, &config));
}

#[test]
fn gate_is_true_even_when_the_headset_already_holds_the_output() {
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&air_pods()), &[air_pods()]);
    assert_eq!(reconcile(&snap, &config()), vec![]);
    assert!(has_eligible_arrival(&snap, &config()));
}

#[test]
fn rule_off_does_nothing() {
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&mac_studio_speakers()), &[air_pods()]);
    let mut config = config();
    config.headphones_take_over = false;
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn two_headphones_arriving_together_resolve_by_name() {
    let snap = snapshot(
        vec![mac_studio_speakers(), air_pods_pro(), air_pods()],
        Some(&mac_studio_speakers()),
        &[air_pods_pro(), air_pods()],
    );
    assert_eq!(
        reconcile(&snap, &config()),
        vec![Action::SetDefaultOutput(air_pods().id, "Mac Studio的揚聲器 -> AirPods Max (headphones connected)".into())]
    );
}

#[test]
fn present_but_not_arrived_headphones_are_left_alone() {
    let snap = snapshot(vec![mac_studio_speakers(), air_pods()], Some(&mac_studio_speakers()), &[]);
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn no_default_output_still_takes_over() {
    let snap = snapshot(vec![air_pods()], None, &[air_pods()]);
    assert_eq!(
        reconcile(&snap, &config()),
        vec![Action::SetDefaultOutput(air_pods().id, "- -> AirPods Max (headphones connected)".into())]
    );
}
