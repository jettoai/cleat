mod common;

use cleat_rs::config::Config;
use cleat_rs::model::{Action, DeviceSnapshot};
use cleat_rs::rules::balance;
use common::*;

fn config() -> Config {
    Config { balance: Some(0.5), ..Config::default() }
}

fn snapshot(balance: Option<f32>, output: Option<u32>) -> DeviceSnapshot {
    DeviceSnapshot { devices: vec![air_pods()], default_output: output, output_balance: balance, ..Default::default() }
}

fn ap() -> Option<u32> {
    Some(air_pods().id)
}

#[test]
fn within_tolerance_does_nothing() {
    assert_eq!(balance::reconcile(&snapshot(Some(0.49), ap()), &config()), vec![]);
}

#[test]
fn drift_is_pulled_back() {
    assert_eq!(
        balance::reconcile(&snapshot(Some(0.3), ap()), &config()),
        vec![Action::SetBalance(air_pods().id, 0.5, "AirPods Max 0.30 -> 0.50".into())]
    );
}

#[test]
fn unreadable_balance_does_nothing() {
    assert_eq!(balance::reconcile(&snapshot(None, ap()), &config()), vec![]);
}

#[test]
fn disabled_config_does_nothing() {
    assert_eq!(balance::reconcile(&snapshot(Some(0.3), ap()), &Config::default()), vec![]);
}

#[test]
fn no_default_output_does_nothing() {
    assert_eq!(balance::reconcile(&snapshot(Some(0.3), None), &config()), vec![]);
}

#[test]
fn non_center_target_is_honoured() {
    assert_eq!(
        balance::reconcile(&snapshot(Some(0.5), ap()), &Config { balance: Some(0.2), ..Config::default() }),
        vec![Action::SetBalance(air_pods().id, 0.2, "AirPods Max 0.50 -> 0.20".into())]
    );
}
