//! Rule 1 (with the rule 2 gate): keep the default input on the most preferred microphone that is
//! plugged in and sending signal, but only move off a device that is itself listed or blocked, or
//! that cleat itself moved to when it evicted a blocked one.
//! A blocked device is never the target, even when it is also on the priority list.
//! A blocked current input is evicted even without a live candidate and even with an empty
//! priority list: first to a present listed device with signal, then to the built-in microphone,
//! then to a present listed device that is silent or still measured (a device cleat placed there
//! gives way once the listed one has signal). Nothing else outside the list is a fallback; without
//! one the device stays.

use crate::config::Config;
use crate::model::{Action, AudioDevice, AudioDeviceId, DeviceSnapshot, Liveness};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let target = config
        .input
        .iter()
        .filter_map(|e| snapshot.device_matching(e, true))
        .filter(|c| !c.is_listed(&config.blocked_input))
        .find(|c| match snapshot.liveness.get(&c.uid) {
            Some(Liveness::Silent) => false,
            Some(Liveness::Measuring) => Some(c.id) == snapshot.default_input,
            Some(Liveness::Live) | None => true,
        });
    let Some(current_id) = snapshot.default_input else { return vec![] };
    let Some(target) = target else { return evict_blocked(current_id, snapshot, config) };
    if current_id == target.id {
        return vec![];
    }
    let Some(current) = snapshot.device(current_id) else { return vec![] };

    let is_pinned = current.is_listed(&config.input);
    let is_blocked = current.is_listed(&config.blocked_input);
    let is_placed = snapshot.placed_input.as_deref() == Some(current.uid.as_str());
    if !is_pinned && !is_blocked && !is_placed {
        return vec![];
    }
    let cause = if is_blocked {
        "blocked".to_string()
    } else if !is_pinned {
        "listed device back".to_string()
    } else if snapshot.liveness.get(&current.uid) == Some(&Liveness::Silent) {
        format!("{} silent", current.name)
    } else {
        "higher priority present".to_string()
    };
    vec![Action::SetDefaultInput(target.id, format!("{} -> {} ({})", current.name, target.name, cause))]
}

fn evict_blocked(current_id: AudioDeviceId, snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let Some(current) = snapshot.device(current_id) else { return vec![] };
    if !current.is_listed(&config.blocked_input) {
        return vec![];
    }
    let usable = |d: &&AudioDevice| !d.is_listed(&config.blocked_input);
    let listed = || config.input.iter().filter_map(|e| snapshot.device_matching(e, true)).filter(usable);
    let has_signal = |d: &&AudioDevice| matches!(snapshot.liveness.get(&d.uid), Some(Liveness::Live) | None);
    let escape = listed()
        .find(has_signal)
        .or_else(|| {
            snapshot
                .devices
                .iter()
                .filter(|d| d.has_input && d.is_built_in())
                .filter(usable)
                .min_by(|a, b| AudioDevice::by_name(a, b))
        })
        .or_else(|| listed().next());
    match escape {
        Some(e) => vec![Action::SetDefaultInput(e.id, format!("{} -> {} (blocked)", current.name, e.name))],
        None => vec![],
    }
}
