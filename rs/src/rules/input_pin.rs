//! Rule 1 (with the rule 2 gate): keep the default input on the most preferred microphone that is
//! plugged in and sending signal, but only move off a device that is itself listed or blocked.

use crate::config::Config;
use crate::model::{Action, DeviceSnapshot, Liveness};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    if config.input.is_empty() {
        return vec![];
    }
    let target = config
        .input
        .iter()
        .filter_map(|e| snapshot.device_matching(e, true))
        .find(|c| match snapshot.liveness.get(&c.uid) {
            Some(Liveness::Silent) => false,
            Some(Liveness::Measuring) => Some(c.id) == snapshot.default_input,
            Some(Liveness::Live) | None => true,
        });
    let Some(target) = target else { return vec![] };
    let Some(current_id) = snapshot.default_input else { return vec![] };
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
