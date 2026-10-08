//! Rule 1 (with the rule 2 gate): keep the default input on the most preferred microphone that is
//! plugged in and sending signal, but only move off a device that is itself listed, or that cleat
//! itself moved to when it evicted a "not used" one. What "not used" means, and what happens when
//! the default input is one, is `rules::blocked`; this rule does nothing on a blocked default.

use super::blocked::{self, Side};
use crate::config::Config;
use crate::model::{Action, DeviceSnapshot, Liveness};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let Some(current_id) = snapshot.default_input else { return vec![] };
    let Some(current) = snapshot.device(current_id) else { return vec![] };
    if blocked::is_blocked(current, Side::Input, config) {
        return vec![];
    }
    let target = config
        .input
        .iter()
        .filter_map(|e| snapshot.device_matching(e, true))
        .filter(|c| !blocked::is_blocked(c, Side::Input, config))
        .find(|c| match snapshot.liveness.get(&c.uid) {
            Some(Liveness::Silent) => false,
            Some(Liveness::Measuring) => c.id == current_id,
            Some(Liveness::Live) | None => true,
        });
    let Some(target) = target else { return vec![] };
    if current_id == target.id {
        return vec![];
    }
    let is_pinned = current.is_listed(&config.input);
    let is_placed = snapshot.placed_input.as_deref() == Some(current.uid.as_str());
    if !is_pinned && !is_placed {
        return vec![];
    }
    let cause = if !is_pinned {
        "listed device back".to_string()
    } else if snapshot.liveness.get(&current.uid) == Some(&Liveness::Silent) {
        format!("{} silent", current.name)
    } else {
        "higher priority present".to_string()
    };
    vec![Action::SetDefaultInput(target.id, format!("{} -> {} ({})", current.name, target.name, cause))]
}
