//! Rule 6: a pair of Bluetooth headphones becomes the output the moment it connects. Edge
//! triggered through `snapshot.arrived`.

use crate::config::Config;
use crate::model::{Action, AudioDevice, DeviceSnapshot};

/// A Bluetooth output while takeover is on, unless blocked.
pub fn owns(device: &AudioDevice, config: &Config) -> bool {
    config.headphones_take_over && device.is_bluetooth() && !device.is_listed(&config.blocked_output)
}

/// The arriving headsets this rule would act on, most preferred (by name) first.
pub fn eligible_arrivals<'a>(snapshot: &'a DeviceSnapshot, config: &Config) -> Vec<&'a AudioDevice> {
    if !config.headphones_take_over {
        return vec![];
    }
    let mut out: Vec<&AudioDevice> = snapshot
        .devices
        .iter()
        .filter(|d| snapshot.arrived.contains(&d.uid) && d.has_output && owns(d, config))
        .collect();
    out.sort_by(|a, b| AudioDevice::by_name(a, b));
    out
}

pub fn has_eligible_arrival(snapshot: &DeviceSnapshot, config: &Config) -> bool {
    !eligible_arrivals(snapshot, config).is_empty()
}

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let Some(target) = eligible_arrivals(snapshot, config).into_iter().next() else { return vec![] };
    if snapshot.default_output == Some(target.id) {
        return vec![];
    }
    let current = snapshot.output_device().map_or("-", |d| d.name.as_str());
    vec![Action::SetDefaultOutput(target.id, format!("{} -> {} (headphones connected)", current, target.name))]
}
