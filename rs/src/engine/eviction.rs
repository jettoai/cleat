//! Eviction bookkeeping (B-1283). Which device cleat itself moved to when it evicted a blocked
//! default (the rules see it as `placed_input`/`placed_output` and do not treat it as a hand
//! pick), and the cooldown that ends a tug of war with macOS: a device evicted
//! `EVICTION_LIMIT` times within `EVICTION_WINDOW` is left in place until a device is added or
//! removed or the config is reloaded. A microphone's signal flipping lifts an input-side pause
//! once per tug of war (B-1287); it never lifts the output side.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use super::Engine;
use crate::model::{Action, DeviceSnapshot};
use crate::rules::blocked::{self, Verdict};
use crate::rules::{input_pin, output_pin};

pub const EVICTION_WINDOW: Duration = Duration::from_secs(60);
pub const EVICTION_LIMIT: usize = 3;

pub(crate) use crate::rules::blocked::Side;

#[derive(Default)]
struct SideBook {
    /// UID cleat moved to on its last eviction, while it is still the default.
    placed: Option<String>,
    /// Pass times of the evictions let through, per evicted UID.
    strikes: HashMap<String, VecDeque<Duration>>,
    /// Evicted UIDs left in place until the next device change or config reload.
    paused: HashSet<String>,
    /// Paused UIDs a signal flip has already lifted once in this tug of war.
    signal_lifted: HashSet<String>,
}

impl SideBook {
    fn lift(&mut self) {
        self.strikes.clear();
        self.paused.clear();
        self.signal_lifted.clear();
    }
}

#[derive(Default)]
pub(crate) struct EvictionBook {
    input: SideBook,
    output: SideBook,
    last_uids: Option<HashSet<String>>,
}

impl EvictionBook {
    /// Start of a pass: a device added or removed lifts every pause; a placed device that is no
    /// longer the default stops counting as placed. Then fills the snapshot's two fields.
    pub(crate) fn observe(&mut self, snap: &mut DeviceSnapshot) {
        let uids: HashSet<String> = snap.devices.iter().map(|d| d.uid.clone()).collect();
        if self.last_uids.as_ref().is_some_and(|prev| *prev != uids) {
            self.input.lift();
            self.output.lift();
        }
        self.last_uids = Some(uids);
        let uid_of = |id: Option<u32>| id.and_then(|i| snap.device(i)).map(|d| d.uid.clone());
        if self.input.placed != uid_of(snap.default_input) {
            self.input.placed = None;
        }
        if self.output.placed != uid_of(snap.default_output) {
            self.output.placed = None;
        }
        snap.placed_input = self.input.placed.clone();
        snap.placed_output = self.output.placed.clone();
    }

    /// The config changed: what the user asks for now is not held back by earlier evictions.
    pub(crate) fn lift_all(&mut self) {
        self.input.lift();
        self.output.lift();
    }

    /// A microphone's signal flipped: each paused input device gets one more round, once per tug
    /// of war (a device change or config reload starts a new one). The output side is untouched.
    pub(crate) fn lift_input_on_signal(&mut self) {
        let book = &mut self.input;
        let fresh: Vec<String> = book.paused.difference(&book.signal_lifted).cloned().collect();
        for uid in fresh {
            book.paused.remove(&uid);
            book.strikes.remove(&uid);
            book.signal_lifted.insert(uid);
        }
    }

    /// Lets one eviction through unless the device it leaves has been evicted `EVICTION_LIMIT`
    /// times in `EVICTION_WINDOW`; records the ones let through. Returns the action if kept and
    /// the log line to write (one, the first time a device is held).
    pub(crate) fn gate(
        &mut self,
        side: Side,
        action: Action,
        snap: &DeviceSnapshot,
        now: Duration,
    ) -> (Option<Action>, Option<String>) {
        let (book, current_id) = match side {
            Side::Input => (&mut self.input, snap.default_input),
            Side::Output => (&mut self.output, snap.default_output),
        };
        let Some(current) = current_id.and_then(|id| snap.device(id)) else { return (Some(action), None) };
        let target = match &action {
            Action::SetDefaultInput(id, _) | Action::SetDefaultOutput(id, _) => *id,
            _ => return (Some(action), None),
        };
        let times = book.strikes.entry(current.uid.clone()).or_default();
        while times.front().is_some_and(|t| now.saturating_sub(*t) >= EVICTION_WINDOW) {
            times.pop_front();
        }
        if book.paused.contains(&current.uid) || times.len() >= EVICTION_LIMIT {
            let until = match side {
                Side::Input => "a device is added or removed, the config reloads, or a microphone's signal changes",
                Side::Output => "a device is added or removed or the config reloads",
            };
            let note = book.paused.insert(current.uid.clone()).then(|| {
                format!(
                    "{}: {} keeps coming back ({EVICTION_LIMIT} evictions in {} s), leaving it until {until}",
                    action.label(),
                    current.name,
                    EVICTION_WINDOW.as_secs()
                )
            });
            return (None, note);
        }
        times.push_back(now);
        book.placed = snap.device(target).map(|d| d.uid.clone());
        (Some(action), None)
    }
}

/// The device each side is left on because it is "not used" and nothing else is usable, and the
/// one the cooldown currently leaves in place instead of evicting.
#[derive(Debug, Default)]
pub(crate) struct Stuck {
    pub input: Option<String>,
    pub output: Option<String>,
    pub input_paused: Option<String>,
    pub output_paused: Option<String>,
}

impl Stuck {
    pub(super) fn paused_mut(&mut self, side: Side) -> &mut Option<String> {
        match side {
            Side::Input => &mut self.input_paused,
            Side::Output => &mut self.output_paused,
        }
    }
}

impl Engine {
    /// One side's default-device decision, in this order: a "not used" default leaves through the
    /// cooldown gate, or is reported stuck; only a default that is not blocked goes to the pin rule.
    pub(super) fn pin_side(
        &mut self,
        side: Side,
        snap: &DeviceSnapshot,
        now: Duration,
        held: &mut Vec<String>,
    ) -> Vec<Action> {
        let (actions, paused) = match blocked::reconcile(side, snap, &self.config) {
            Verdict::Clear => {
                self.set_stuck(side, None, held);
                let actions = match side {
                    Side::Input => input_pin::reconcile(snap, &self.config),
                    Side::Output => output_pin::reconcile(snap, &self.config),
                };
                (actions, None)
            }
            Verdict::Evict(action) => {
                self.set_stuck(side, None, held);
                let (kept, note) = self.eviction.gate(side, action, snap, now);
                held.extend(note);
                let current = match side {
                    Side::Input => snap.default_input,
                    Side::Output => snap.default_output,
                };
                let paused = kept.is_none().then(|| current.and_then(|id| snap.device(id)).map(|d| d.name.clone()));
                (kept.into_iter().collect(), paused.flatten())
            }
            Verdict::Stuck(device) => {
                self.set_stuck(side, Some(device.name), held);
                (vec![], None)
            }
        };
        *self.stuck.paused_mut(side) = paused;
        actions
    }

    /// Edge triggered: one line when a side becomes stuck on a "not used" device, one when it stops.
    pub(super) fn set_stuck(&mut self, side: Side, name: Option<String>, held: &mut Vec<String>) {
        let (slot, label) = match side {
            Side::Input => (&mut self.stuck.input, "pinInput"),
            Side::Output => (&mut self.stuck.output, "pinOutput"),
        };
        if *slot == name {
            return;
        }
        match (&*slot, &name) {
            (_, Some(n)) => held.push(format!("{label}: no other usable device, still on {n}")),
            (Some(old), None) => held.push(format!("{label}: {old} no longer the default")),
            (None, None) => {}
        }
        *slot = name;
    }
}
