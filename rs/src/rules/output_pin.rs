//! Rule 4: the priority list for outputs. A Bluetooth output is outside this rule while headphone
//! takeover is on. A blocked current output is evicted even when the list is empty or has run
//! out, to the Mac's built-in output and nothing else; without one it stays.
//! A blocked device is never the target, even when it is also on the priority list. An output
//! cleat itself moved to when it evicted a blocked one gives way to a listed device that comes back.

use super::headphones_takeover::owns;
use crate::config::Config;
use crate::model::{Action, AudioDevice, DeviceSnapshot};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let listed: Vec<&AudioDevice> =
        config.output.iter().filter_map(|e| snapshot.device_matching(e, false)).collect();
    let Some(current_id) = snapshot.default_output else { return vec![] };
    let Some(current) = snapshot.device(current_id) else { return vec![] };

    let Some(target) = listed.iter().find(|d| !owns(d, config) && !d.is_listed(&config.blocked_output)) else {
        return evict_blocked(current, snapshot, config);
    };
    if current_id == target.id || owns(current, config) {
        return vec![];
    }
    let is_pinned = current.is_listed(&config.output);
    let is_blocked = current.is_listed(&config.blocked_output);
    let is_placed = snapshot.placed_output.as_deref() == Some(current.uid.as_str());
    if !is_pinned && !is_blocked && !is_placed {
        return vec![];
    }
    let cause = if is_blocked {
        "blocked"
    } else if !is_pinned {
        "listed device back"
    } else {
        "higher priority present"
    };
    vec![Action::SetDefaultOutput(target.id, format!("{} -> {} ({})", current.name, target.name, cause))]
}

fn evict_blocked(current: &AudioDevice, snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    if !current.is_listed(&config.blocked_output) {
        return vec![];
    }
    let escape = snapshot
        .devices
        .iter()
        .filter(|d| d.has_output && d.is_built_in() && !d.is_listed(&config.blocked_output) && !owns(d, config))
        .min_by(|a, b| AudioDevice::by_name(a, b));
    match escape {
        Some(e) => vec![Action::SetDefaultOutput(e.id, format!("{} -> {} (blocked)", current.name, e.name))],
        None => vec![],
    }
}
