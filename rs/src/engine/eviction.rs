//! Eviction bookkeeping (B-1283). Which device cleat itself moved to when it evicted a blocked
//! default (the rules see it as `placed_input`/`placed_output` and do not treat it as a hand
//! pick), and the cooldown that ends a tug of war with macOS: a device evicted
//! `EVICTION_LIMIT` times within `EVICTION_WINDOW` is left in place until a device is added or
//! removed, or the config is reloaded.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use crate::config::Config;
use crate::model::{Action, DeviceSnapshot};

pub const EVICTION_WINDOW: Duration = Duration::from_secs(60);
pub const EVICTION_LIMIT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Input,
    Output,
}

#[derive(Default)]
struct SideBook {
    /// UID cleat moved to on its last eviction, while it is still the default.
    placed: Option<String>,
    /// Pass times of the evictions let through, per evicted UID.
    strikes: HashMap<String, VecDeque<Duration>>,
    /// Evicted UIDs left in place until the next device change or config reload.
    paused: HashSet<String>,
}

impl SideBook {
    fn lift(&mut self) {
        self.strikes.clear();
        self.paused.clear();
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
    pub(crate) fn config_changed(&mut self) {
        self.input.lift();
        self.output.lift();
    }

    /// Lets through a move off a blocked default unless that device has been evicted
    /// `EVICTION_LIMIT` times in `EVICTION_WINDOW`; records the ones let through. Returns the
    /// actions kept and the log lines to write (one, the first time a device is held).
    pub(crate) fn gate(
        &mut self,
        side: Side,
        actions: Vec<Action>,
        snap: &DeviceSnapshot,
        config: &Config,
        now: Duration,
    ) -> (Vec<Action>, Vec<String>) {
        let (book, current_id, blocked) = match side {
            Side::Input => (&mut self.input, snap.default_input, &config.blocked_input),
            Side::Output => (&mut self.output, snap.default_output, &config.blocked_output),
        };
        let Some(current) = current_id.and_then(|id| snap.device(id)).filter(|d| d.is_listed(blocked)) else {
            return (actions, vec![]);
        };
        let mut kept = Vec::with_capacity(actions.len());
        let mut notes = vec![];
        for action in actions {
            let target = match (&action, side) {
                (Action::SetDefaultInput(id, _), Side::Input) | (Action::SetDefaultOutput(id, _), Side::Output) => *id,
                _ => {
                    kept.push(action);
                    continue;
                }
            };
            let times = book.strikes.entry(current.uid.clone()).or_default();
            while times.front().is_some_and(|t| now.saturating_sub(*t) >= EVICTION_WINDOW) {
                times.pop_front();
            }
            if book.paused.contains(&current.uid) || times.len() >= EVICTION_LIMIT {
                if book.paused.insert(current.uid.clone()) {
                    notes.push(format!(
                        "{}: {} keeps coming back ({EVICTION_LIMIT} evictions in {} s), leaving it until a device is added or removed",
                        action.label(),
                        current.name,
                        EVICTION_WINDOW.as_secs()
                    ));
                }
                continue;
            }
            times.push_back(now);
            book.placed = snap.device(target).map(|d| d.uid.clone());
            kept.push(action);
        }
        (kept, notes)
    }
}
