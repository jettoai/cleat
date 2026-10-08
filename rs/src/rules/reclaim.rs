//! Rule 7: ask a listed, connected headset back while the Mac is playing and it is not the output.

use std::collections::HashSet;

use crate::config::Config;
use crate::model::{device_name, Action, BluetoothHeadset, DeviceSnapshot};

pub fn request_reason(headset: &BluetoothHeadset) -> String {
    format!("cleat: {} connected but not an audio device, Mac is playing", headset.name)
}

/// `excluding` holds addresses the engine is waiting on or backing off from.
pub fn reconcile(
    snapshot: &DeviceSnapshot,
    headsets: &[BluetoothHeadset],
    config: &Config,
    excluding: &HashSet<String>,
) -> Vec<Action> {
    if config.reclaim_active().is_empty() || !snapshot.output_running {
        return vec![];
    }
    let Some(target) = candidates(snapshot, headsets, config).into_iter().find(|h| !excluding.contains(&h.address))
    else {
        return vec![];
    };
    vec![Action::RequestRoute {
        name: target.name.clone(),
        address: target.address.clone(),
        reason: request_reason(&target),
    }]
}

pub fn candidates(snapshot: &DeviceSnapshot, headsets: &[BluetoothHeadset], config: &Config) -> Vec<BluetoothHeadset> {
    in_rule_order(
        headsets
            .iter()
            .filter(|h| h.is_connected && h.is_listed(config.reclaim_active()) && !is_default_output(h, snapshot))
            .cloned()
            .collect(),
    )
}

/// Name first, address as the tie break.
pub fn in_rule_order(mut headsets: Vec<BluetoothHeadset>) -> Vec<BluetoothHeadset> {
    headsets.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.address.cmp(&b.address)));
    headsets
}

pub fn is_audio_device(headset: &BluetoothHeadset, snapshot: &DeviceSnapshot) -> bool {
    snapshot.devices.iter().any(|d| d.has_output && device_name::matches(&headset.name, &d.name, &d.uid))
}

pub fn is_default_output(headset: &BluetoothHeadset, snapshot: &DeviceSnapshot) -> bool {
    snapshot.output_device().is_some_and(|d| device_name::matches(&headset.name, &d.name, &d.uid))
}
