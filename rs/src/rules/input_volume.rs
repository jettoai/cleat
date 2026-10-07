//! Rule 5: hold each configured microphone's input gain. Walks devices (so `"*"` reaches unnamed
//! ones); a device whose gain could not be read is skipped.

use crate::config::Config;
use crate::model::{Action, AudioDevice, DeviceSnapshot};

/// Percent.
pub const TOLERANCE: f64 = 0.5;

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    if config.input_volume.is_empty() {
        return vec![];
    }
    let mut devices: Vec<&AudioDevice> = snapshot.devices.iter().filter(|d| d.has_input).collect();
    devices.sort_by(|a, b| AudioDevice::by_name(a, b));
    devices
        .into_iter()
        .filter_map(|d| {
            let wanted = config.input_volume_target(d)?;
            let current = *snapshot.input_volumes.get(&d.id)? as f64 * 100.0;
            if (current - wanted).abs() <= TOLERANCE {
                return None;
            }
            Some(Action::SetInputVolume(
                d.id,
                (wanted / 100.0) as f32,
                format!("{} {:.0}% -> {:.0}%", d.name, current, wanted),
            ))
        })
        .collect()
}
