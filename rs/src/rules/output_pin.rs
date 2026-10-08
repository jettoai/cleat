//! Rule 4: the priority list for outputs. A Bluetooth output is outside this rule while headphone
//! takeover is on. A "not used" default output is `rules::blocked`'s case, never this rule's. An
//! output cleat itself moved to when it evicted one gives way to a listed device that comes back.

use super::blocked::{self, Side};
use super::headphones_takeover::owns;
use crate::config::Config;
use crate::model::{Action, DeviceSnapshot};

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let Some(current_id) = snapshot.default_output else { return vec![] };
    let Some(current) = snapshot.device(current_id) else { return vec![] };
    if blocked::is_blocked(current, Side::Output, config) || owns(current, config) {
        return vec![];
    }
    let target = config
        .output
        .iter()
        .filter_map(|e| snapshot.device_matching(e, false))
        .find(|d| !owns(d, config) && !blocked::is_blocked(d, Side::Output, config));
    let Some(target) = target else { return vec![] };
    if current_id == target.id {
        return vec![];
    }
    let is_pinned = current.is_listed(&config.output);
    let is_placed = snapshot.placed_output.as_deref() == Some(current.uid.as_str());
    if !is_pinned && !is_placed {
        return vec![];
    }
    let cause = if !is_pinned { "listed device back" } else { "higher priority present" };
    vec![Action::SetDefaultOutput(target.id, format!("{} -> {} ({})", current.name, target.name, cause))]
}
