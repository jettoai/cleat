//! The "not used" lists (B-1287): the one place that says what a blocked device is, and what
//! happens when a side's default is one. Pure, like every rule.
//!
//! Two halves. Never a target: `is_blocked` is the predicate the pin rules and headphone takeover
//! consult when they pick a device; reclaim works on Bluetooth headsets, not `AudioDevice`s, and
//! checks the same `blockedOutput` list itself (`BluetoothHeadset::is_listed`, which matches
//! through `device_name::matches` as `is_blocked` does, plus the headset's address). Always moved off:
//! `reconcile` sends a blocked default to the first device in `escape`, and says `Stuck` when
//! there is nowhere to go, so the user can be told instead of guessing.

use super::headphones_takeover::owns;
use crate::config::Config;
use crate::model::{Action, AudioDevice, DeviceSnapshot, Liveness, TRANSPORT_UNKNOWN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Input,
    Output,
}

/// What the rule found for one side.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The default is not blocked: the side's pin rule decides.
    Clear,
    /// The default is blocked and leaves for the device in the action.
    Evict(Action),
    /// The default is blocked and no other usable device is present: it stays, and is reported.
    Stuck(AudioDevice),
}

pub fn blocked_list(side: Side, config: &Config) -> &[String] {
    match side {
        Side::Input => &config.blocked_input,
        Side::Output => &config.blocked_output,
    }
}

/// The one definition of "not used" on a side, by name or UID (`device_name::matches`).
pub fn is_blocked(device: &AudioDevice, side: Side, config: &Config) -> bool {
    device.is_listed(blocked_list(side, config))
}

/// A device the side may be moved onto: has that side, not blocked, and for outputs not a
/// headset that takeover owns (the list never moves the sound onto one by itself).
fn selectable(device: &AudioDevice, side: Side, config: &Config) -> bool {
    let on_side = match side {
        Side::Input => device.has_input,
        Side::Output => device.has_output,
    };
    on_side && !is_blocked(device, side, config) && (side == Side::Input || !owns(device, config))
}

/// Where a blocked default goes (PM rulings 2026-10-09, B-1287), in this order and nothing else.
/// Input:
///   1. a listed, selectable microphone with signal (`Live`, or untracked), in config order,
///      which is exactly the pin rule's own target;
///   2. the built-in microphone (several: by name);
///   3. a listed, selectable microphone that is silent or still measured, in config order (it
///      gives way to a listed one with signal once that one has it, `placed_input`).
///
/// An unlisted microphone is never an escape. Output:
///   1. a listed, selectable output, in config order;
///   2. any selectable physical output (`AudioDevice::is_physical`), built-in first, then by name.
///
/// None means the blocked default stays (`Verdict::Stuck`).
pub fn escape<'a>(side: Side, snapshot: &'a DeviceSnapshot, config: &Config) -> Option<&'a AudioDevice> {
    let (list, input) = match side {
        Side::Input => (&config.input, true),
        Side::Output => (&config.output, false),
    };
    let listed = || {
        list.iter().filter_map(move |e| snapshot.device_matching(e, input)).filter(move |d| selectable(d, side, config))
    };
    let has_signal = |d: &&AudioDevice| matches!(snapshot.liveness.get(&d.uid), Some(Liveness::Live) | None);
    let pool = |physical: fn(&AudioDevice) -> bool| {
        snapshot
            .devices
            .iter()
            .filter(move |d| selectable(d, side, config) && physical(d))
            .min_by(|a, b| b.is_built_in().cmp(&a.is_built_in()).then_with(|| AudioDevice::by_name(a, b)))
    };
    match side {
        Side::Input => listed()
            .find(has_signal)
            .or_else(|| pool(AudioDevice::is_built_in))
            .or_else(|| listed().find(|d| !has_signal(d))),
        Side::Output => listed().next().or_else(|| pool(AudioDevice::is_physical)),
    }
}

pub fn reconcile(side: Side, snapshot: &DeviceSnapshot, config: &Config) -> Verdict {
    let current_id = match side {
        Side::Input => snapshot.default_input,
        Side::Output => snapshot.default_output,
    };
    let Some(current) = current_id.and_then(|id| snapshot.device(id)) else { return Verdict::Clear };
    // A blocked virtual, aggregate, Continuity or AirPlay device (a meeting app's own) is only
    // never chosen: an app that switches to it itself is not overruled, as in the Swift version
    // (PM ruling 2026-10-09, B-1287). An unknown transport (the HAL failed to say) is still moved
    // off, so a headset that once reads as unknown is not left silently in place.
    if !is_blocked(current, side, config) || (!current.is_physical() && current.transport != TRANSPORT_UNKNOWN) {
        return Verdict::Clear;
    }
    match escape(side, snapshot, config) {
        Some(e) => {
            let reason = format!("{} -> {} (blocked)", current.name, e.name);
            Verdict::Evict(match side {
                Side::Input => Action::SetDefaultInput(e.id, reason),
                Side::Output => Action::SetDefaultOutput(e.id, reason),
            })
        }
        None => Verdict::Stuck(current.clone()),
    }
}
