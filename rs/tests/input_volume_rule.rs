mod common;

use std::collections::HashMap;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, AudioDevice, DeviceSnapshot};
use cleat_rs::rules::input_volume::reconcile;
use common::*;

fn config() -> Config {
    Config {
        input: s(&["Wireless microphone", "Brio 100"]),
        input_volume: volumes(&[("Wireless microphone", 88.0), ("Brio 100", 75.0)]),
        ..Config::default()
    }
}

fn wildcard_with_override() -> Config {
    Config { input_volume: volumes(&[("*", 100.0), ("Brio 100", 75.0)]), ..Config::default() }
}

fn wildcard() -> Config {
    Config { input_volume: volumes(&[("*", 100.0)]), ..Config::default() }
}

fn vols(v: &[(u32, f32)]) -> HashMap<u32, f32> {
    v.iter().copied().collect()
}

fn snap(devices: Vec<AudioDevice>, default_input: Option<u32>, v: &[(u32, f32)]) -> DeviceSnapshot {
    DeviceSnapshot { devices, default_input, input_volumes: vols(v), ..Default::default() }
}

fn snapshot(v: &[(u32, f32)]) -> DeviceSnapshot {
    snap(vec![wireless(), brio(), maono()], Some(wireless().id), v)
}

#[test]
fn matching_volume_does_nothing() {
    assert_eq!(reconcile(&snapshot(&[(wireless().id, 0.88), (brio().id, 0.75)]), &config()), vec![]);
}

#[test]
fn drifted_volume_is_pulled_back() {
    assert_eq!(
        reconcile(&snapshot(&[(wireless().id, 0.60), (brio().id, 0.75)]), &config()),
        vec![Action::SetInputVolume(wireless().id, 0.88, "Wireless microphone 60% -> 88%".into())]
    );
}

#[test]
fn both_devices_sharing_a_name_are_held() {
    let s = snap(vec![brio(), second_brio()], Some(brio().id), &[(brio().id, 0.40), (second_brio().id, 0.50)]);
    assert_eq!(
        reconcile(&s, &config()),
        vec![
            Action::SetInputVolume(brio().id, 0.75, "Brio 100 40% -> 75%".into()),
            Action::SetInputVolume(second_brio().id, 0.75, "Brio 100 50% -> 75%".into()),
        ]
    );
}

#[test]
fn non_default_device_is_also_held() {
    let s = snap(vec![wireless(), brio(), maono()], Some(wireless().id), &[(wireless().id, 0.88), (brio().id, 0.40)]);
    assert_eq!(reconcile(&s, &config()), vec![Action::SetInputVolume(brio().id, 0.75, "Brio 100 40% -> 75%".into())]);
}

#[test]
fn unlisted_device_is_left_alone() {
    let s = snapshot(&[(wireless().id, 0.88), (brio().id, 0.75), (maono().id, 0.10)]);
    assert_eq!(reconcile(&s, &config()), vec![]);
}

#[test]
fn device_with_no_reading_is_skipped() {
    assert_eq!(reconcile(&snapshot(&[(brio().id, 0.75)]), &config()), vec![]);
}

#[test]
fn absent_device_is_skipped() {
    let s = snap(vec![brio()], Some(brio().id), &[(brio().id, 0.40)]);
    assert_eq!(reconcile(&s, &config()), vec![Action::SetInputVolume(brio().id, 0.75, "Brio 100 40% -> 75%".into())]);
}

#[test]
fn multiple_drifts_are_ordered_deterministically() {
    assert_eq!(
        reconcile(&snapshot(&[(wireless().id, 0.10), (brio().id, 0.10)]), &config()),
        vec![
            Action::SetInputVolume(brio().id, 0.75, "Brio 100 10% -> 75%".into()),
            Action::SetInputVolume(wireless().id, 0.88, "Wireless microphone 10% -> 88%".into()),
        ]
    );
}

#[test]
fn empty_config_does_nothing() {
    assert_eq!(reconcile(&snapshot(&[(wireless().id, 0.10)]), &Config::default()), vec![]);
}

#[test]
fn named_entry_overrides_the_wildcard() {
    let s = snap(vec![brio()], Some(brio().id), &[(brio().id, 0.40)]);
    assert_eq!(
        reconcile(&s, &wildcard_with_override()),
        vec![Action::SetInputVolume(brio().id, 0.75, "Brio 100 40% -> 75%".into())]
    );
}

#[test]
fn unnamed_device_falls_back_to_the_wildcard() {
    let s = snap(vec![air_pods()], Some(air_pods().id), &[(air_pods().id, 0.50)]);
    assert_eq!(
        reconcile(&s, &wildcard_with_override()),
        vec![Action::SetInputVolume(air_pods().id, 1.0, "AirPods Max 50% -> 100%".into())]
    );
}

#[test]
fn without_wildcard_an_unnamed_device_is_untouched() {
    let config = Config { input_volume: volumes(&[("Brio 100", 75.0)]), ..Config::default() };
    let s = snap(vec![air_pods()], Some(air_pods().id), &[(air_pods().id, 0.50)]);
    assert_eq!(reconcile(&s, &config), vec![]);
}

#[test]
fn wildcard_skips_a_device_with_no_reading() {
    let s = snap(vec![zoom(), brio()], Some(brio().id), &[(brio().id, 1.0)]);
    assert_eq!(reconcile(&s, &wildcard()), vec![]);
}

#[test]
fn wildcard_actions_are_ordered_by_device_name() {
    let s = snap(
        vec![wireless(), brio(), air_pods(), display_speakers()],
        Some(brio().id),
        &[(wireless().id, 0.10), (brio().id, 0.20), (air_pods().id, 0.30)],
    );
    assert_eq!(
        reconcile(&s, &wildcard()),
        vec![
            Action::SetInputVolume(air_pods().id, 1.0, "AirPods Max 30% -> 100%".into()),
            Action::SetInputVolume(brio().id, 1.0, "Brio 100 20% -> 100%".into()),
            Action::SetInputVolume(wireless().id, 1.0, "Wireless microphone 10% -> 100%".into()),
        ]
    );
}

#[test]
fn wildcard_ignores_output_only_devices() {
    let s = snap(vec![display_speakers()], None, &[(display_speakers().id, 0.20)]);
    assert_eq!(reconcile(&s, &wildcard()), vec![]);
}

#[test]
fn wildcard_leaves_a_matched_device_alone() {
    let s = snap(vec![brio()], Some(brio().id), &[(brio().id, 1.0)]);
    assert_eq!(reconcile(&s, &wildcard()), vec![]);
}
