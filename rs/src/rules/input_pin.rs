//! Rule 1 (with the rule 2 gate): keep the default input on the most preferred microphone that is
//! plugged in and sending signal, but only move off a device that is itself listed or blocked.
//! A blocked device is never the target, even when it is also on the priority list.
//! A blocked current input is evicted even without a live candidate: first to a present listed
//! device regardless of liveness, then to any other non-blocked input, as rule 4 does for outputs.

use crate::config::Config;
use crate::model::{Action, AudioDevice, AudioDeviceId, DeviceSnapshot, Liveness};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    if config.input.is_empty() {
        return vec![];
    }
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
    if !is_pinned && !is_blocked {
        return vec![];
    }
    let cause = if is_blocked {
        "blocked".to_string()
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
    let escape = config
        .input
        .iter()
        .filter_map(|e| snapshot.device_matching(e, true))
        .find(usable)
        .or_else(|| snapshot.devices.iter().filter(|d| d.has_input).filter(usable).min_by(|a, b| AudioDevice::by_name(a, b)));
    match escape {
        Some(e) => vec![Action::SetDefaultInput(e.id, format!("{} -> {} (blocked)", current.name, e.name))],
        None => vec![],
    }
}
