mod common;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot, TRANSPORT_USB};
use cleat_rs::rules::output_pin::reconcile;
use common::*;

fn config() -> Config {
    Config { output: s(&["Studio Display Speakers", "MacBook Pro Speakers"]), ..Config::default() }
}

fn maono_speakers() -> AudioDevice {
    AudioDevice::with_transport(51, "Maono\u{00A0}AI Microphone", "Maono-Output-UID", false, true, TRANSPORT_USB)
}

fn snapshot(devices: Vec<AudioDevice>, current: Option<&AudioDevice>) -> DeviceSnapshot {
    DeviceSnapshot { devices, default_output: current.map(|d| d.id), ..Default::default() }
}

fn out(id: u32, reason: &str) -> Vec<Action> {
    vec![Action::SetDefaultOutput(id, reason.into())]
}

#[test]
fn already_on_target_does_nothing() {
    let snap = snapshot(vec![display_speakers(), mac_speakers()], Some(&display_speakers()));
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn higher_priority_device_takes_over() {
    let snap = snapshot(vec![display_speakers(), mac_speakers()], Some(&mac_speakers()));
    assert_eq!(
        reconcile(&snap, &config()),
        out(display_speakers().id, "MacBook Pro Speakers -> Studio Display Speakers (higher priority present)")
    );
}

#[test]
fn device_outside_the_list_is_left_alone() {
    let snap = snapshot(vec![air_pods(), display_speakers()], Some(&air_pods()));
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn empty_list_disables_the_rule() {
    let snap = snapshot(vec![display_speakers(), mac_speakers()], Some(&mac_speakers()));
    assert_eq!(reconcile(&snap, &Config::default()), vec![]);
}

#[test]
fn no_listed_device_present_does_nothing() {
    let snap = snapshot(vec![air_pods()], Some(&air_pods()));
    assert_eq!(reconcile(&snap, &config()), vec![]);
}

#[test]
fn input_only_device_is_not_an_output_candidate() {
    let config = Config { output: s(&["Brio 100", "MacBook Pro Speakers"]), ..Config::default() };
    let snap = snapshot(vec![brio(), mac_speakers(), display_speakers()], Some(&display_speakers()));
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn blocked_current_output_is_replaced_by_the_first_listed_device() {
    let mut config = config();
    config.blocked_output = s(&["Maono AI Microphone"]);
    let snap = snapshot(vec![maono_speakers(), display_speakers(), mac_speakers()], Some(&maono_speakers()));
    assert_eq!(
        reconcile(&snap, &config),
        out(display_speakers().id, "Maono\u{00A0}AI Microphone -> Studio Display Speakers (blocked)")
    );
}

#[test]
fn blocked_current_output_with_no_listed_device_present_does_nothing() {
    let mut config = config();
    config.blocked_output = s(&["Maono AI Microphone"]);
    let snap = snapshot(vec![maono_speakers()], Some(&maono_speakers()));
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn unlisted_unblocked_current_output_is_still_left_alone() {
    let mut config = config();
    config.blocked_output = s(&["Maono AI Microphone"]);
    let snap = snapshot(vec![air_pods(), display_speakers()], Some(&air_pods()));
    assert_eq!(reconcile(&snap, &config), vec![]);
}

#[test]
fn takeover_on_leaves_a_bluetooth_output_alone() {
    let mut config = config();
    config.output = s(&["Studio Display Speakers", "AirPods Max"]);
    let snap = snapshot(vec![air_pods(), display_speakers()], Some(&air_pods()));

    config.headphones_take_over = true;
    assert_eq!(reconcile(&snap, &config), vec![]);

    config.headphones_take_over = false;
    assert_eq!(
        reconcile(&snap, &config),
        out(display_speakers().id, "AirPods Max -> Studio Display Speakers (higher priority present)")
    );
}

#[test]
fn takeover_on_does_not_switch_to_a_listed_bluetooth_device() {
    let mut config = config();
    config.output = s(&["AirPods Max", "Studio Display Speakers", "MacBook Pro Speakers"]);
    config.headphones_take_over = true;
    let snap = snapshot(vec![air_pods(), display_speakers(), mac_speakers()], Some(&mac_speakers()));
    assert_eq!(
        reconcile(&snap, &config),
        out(display_speakers().id, "MacBook Pro Speakers -> Studio Display Speakers (higher priority present)")
    );
}

#[test]
fn blocked_bluetooth_device_is_still_replaced_with_takeover_on() {
    let mut config = config();
    config.headphones_take_over = true;
    config.blocked_output = s(&["AirPods Max"]);
    let snap = snapshot(vec![air_pods(), display_speakers()], Some(&air_pods()));
    assert_eq!(reconcile(&snap, &config), out(display_speakers().id, "AirPods Max -> Studio Display Speakers (blocked)"));
}

fn headset_only_config() -> Config {
    Config {
        output: s(&["AirPods Max"]),
        headphones_take_over: true,
        blocked_output: s(&["Maono AI Microphone"]),
        ..Config::default()
    }
}

#[test]
fn blocked_current_output_is_evicted_when_every_listed_output_is_a_headset() {
    let snap = snapshot(vec![maono_speakers(), air_pods(), display_speakers()], Some(&maono_speakers()));
    assert_eq!(
        reconcile(&snap, &headset_only_config()),
        out(display_speakers().id, "Maono\u{00A0}AI Microphone -> Studio Display Speakers (blocked)")
    );
}

#[test]
fn blocked_current_output_stays_when_the_only_escape_is_a_headset() {
    let snap = snapshot(vec![maono_speakers(), air_pods()], Some(&maono_speakers()));
    assert_eq!(reconcile(&snap, &headset_only_config()), vec![]);
}

#[test]
fn unlisted_current_output_is_left_alone_when_every_listed_output_is_a_headset() {
    let snap = snapshot(vec![mac_speakers(), air_pods(), display_speakers()], Some(&mac_speakers()));
    assert_eq!(reconcile(&snap, &headset_only_config()), vec![]);
}

#[test]
fn no_default_output_does_nothing() {
    let snap = snapshot(vec![display_speakers(), mac_speakers()], None);
    assert_eq!(reconcile(&snap, &config()), vec![]);
}
