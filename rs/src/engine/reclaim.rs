//! Rule 7's engine side (Swift `Engine+Reclaim.swift`, 0.3.11): when to ask, how often, and what
//! to do with the answer. Log lines are word for word, with two B-1173 changes that are Rust's
//! own: "someone at the Mac" is `model::presence::judge` (keyboard in the last 30 s, or the front
//! app playing a video; a display held awake no longer counts), and every accepted hijack logs
//! how long the headset took to become the output.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use super::Engine;
use crate::model::presence::judge;
use crate::model::{Action, AudioDevice, BluetoothHeadset, DeviceSnapshot};
use crate::reclaim::{Outcome, RouteResponse};
use crate::rules::reclaim;

/// A general playback session. Beats an idle phone (100), loses to media (301) or a call (501).
pub const RECLAIM_SCORE: i32 = 201;
/// Slowest sensible retry against the same headset.
pub const RECLAIM_INTERVAL: Duration = Duration::from_secs(30);
/// How long to leave a headset alone after the phone has won it.
pub const RECLAIM_BACKOFF: Duration = Duration::from_secs(60);
/// A short-lived refusal (out of ear, screen locked) is retried this soon, for at most the span.
pub const RECLAIM_RETRY_DELAY: Duration = Duration::from_secs(8);
pub const RECLAIM_RETRY_SPAN: Duration = Duration::from_secs(180);
/// An output change this close to the last input event was made by hand.
pub const MANUAL_CHOICE_WINDOW_S: f64 = 5.0;
/// How long Cleat's own default-output write counts as the cause of the next output change.
pub const OWN_OUTPUT_WRITE_WINDOW: Duration = Duration::from_secs(10);
/// An accepted hijack whose headset is not the output this long after the request is logged as
/// not back (B-1173).
pub const RETURN_TIMEOUT: Duration = Duration::from_secs(10);
/// A macOS switch back slower than this is a headset taken out of its case, not a return.
pub const MACOS_RETURN_LIMIT: Duration = Duration::from_secs(30);
/// The beats a device change schedules; a granted hijack schedules the same ones.
const RETRY_BEATS_MS: [u64; 3] = [1000, 3000, 6000];

/// A request on its way: when it went out, whether it was accepted, when the headset arrived.
struct Return {
    name: String,
    sent: Duration,
    accepted: bool,
    arrived: Option<Duration>,
}

/// What reclaim remembers between passes (Swift's five engine fields plus `ReclaimWatch`).
#[derive(Default)]
pub struct ReclaimBook {
    next_attempt: HashMap<String, Duration>,
    held_logged: HashMap<String, String>,
    retry_window: HashMap<String, Duration>,
    names: BTreeMap<String, String>,
    asked_this_playback: HashSet<String>,
    unavailable_logged: bool,
    last_output: Option<AudioDevice>,
    user_chose: HashSet<String>,
    own_output_write: Option<(u32, Duration)>,
    not_asking_logged: Option<String>,
    returns: HashMap<String, Return>,
    /// When a listed headset, not the output, was first seen while the Mac played (B-1173).
    macos_return: HashMap<String, Duration>,
}

fn short_lived(detail: &str) -> bool {
    let d = detail.to_lowercase();
    d.contains("out of ear") || d.contains("screen locked")
}

fn headset(name: &str, address: &str) -> BluetoothHeadset {
    BluetoothHeadset { name: name.into(), address: address.into(), is_connected: true }
}

impl Engine {
    fn now(&self) -> Duration {
        self.clock.mono()
    }

    /// The requests this pass should send.
    pub(crate) fn reclaim_requests(&mut self, snap: &DeviceSnapshot) -> Vec<Action> {
        if self.config.reclaim_active().is_empty() {
            return vec![];
        }
        self.forget_headsets_that_came_back(snap);
        self.note_output_departure(snap);
        if !self.routing.is_available() {
            if !self.reclaim.unavailable_logged {
                self.reclaim.unavailable_logged = true;
                self.note("reclaim: unavailable on this macOS");
            }
            return vec![];
        }
        // Asked before the pairing list, which costs a subprocess.
        if !snap.output_running {
            self.reclaim.retry_window.clear();
            self.reclaim.asked_this_playback.clear();
            self.reclaim.user_chose.clear();
            return vec![];
        }

        let headsets = self.inventory.paired_headsets();
        for h in headsets.iter().filter(|h| h.is_listed(self.config.reclaim_active())) {
            self.reclaim.names.insert(h.address.clone(), h.name.clone());
        }
        let candidates = reclaim::candidates(snap, &headsets, &self.config);
        let Some(first) = candidates.first().cloned() else {
            self.reclaim.not_asking_logged = None;
            return vec![];
        };

        let chose = self.present_headsets(&self.reclaim.user_chose, snap);
        let settled = self.present_headsets(&self.reclaim.asked_this_playback, snap);
        let Some(next) = candidates.iter().find(|h| !chose.contains(&h.address) && !settled.contains(&h.address)).cloned()
        else {
            let output = snap.default_output.and_then(|id| snap.device(id)).map_or("another output", |d| d.name.as_str());
            let reason = if chose.contains(&first.address) {
                format!("the user picked {output}")
            } else {
                "already asked this playback".to_string()
            };
            self.note_not_asking(&first, &reason, &reason);
            return vec![];
        };

        let mut facts = snap.presence.clone();
        self.system.complete_presence(&mut facts);
        if let Err(absence) = judge(&facts) {
            self.note_not_asking(&next, &absence.text, &absence.key);
            // Coming back to the keyboard is not a CoreAudio event: a beat of its own.
            self.schedule_reconcile(RECLAIM_RETRY_DELAY.as_millis() as u64);
            return vec![];
        }

        let mut excluding = self.held_down_headsets();
        excluding.extend(chose);
        excluding.extend(settled);
        reclaim::reconcile(snap, &headsets, &self.config, &excluding)
    }

    /// The output moving off a listed headset that is still in CoreAudio, by hand and not by
    /// Cleat: the user chose, and the rest of this playback leaves that alone.
    fn note_output_departure(&mut self, snap: &DeviceSnapshot) {
        let current = snap.default_output.and_then(|id| snap.device(id)).cloned();
        let left = std::mem::replace(&mut self.reclaim.last_output, current.clone());
        let Some(left) = left else { return };
        if current.as_ref().map(|c| &c.uid) == Some(&left.uid) {
            return;
        }
        let Some((address, name)) = self
            .reclaim
            .names
            .iter()
            .find(|(_, n)| crate::model::device_name::matches(n, &left.name, &left.uid))
            .map(|(a, n)| (a.clone(), n.clone()))
        else {
            return;
        };
        if !reclaim::is_audio_device(&headset(&name, &address), snap) {
            return;
        }
        let by_cleat = self.reclaim.own_output_write.is_some_and(|(device, at)| {
            Some(device) == current.as_ref().map(|c| c.id) && self.now().saturating_sub(at) < OWN_OUTPUT_WRITE_WINDOW
        });
        // No reading is the old behaviour: a switch away from a headset in use stood.
        let by_hand = snap.presence.input_idle.is_none_or(|s| s < MANUAL_CHOICE_WINDOW_S);
        if by_hand && !by_cleat {
            self.reclaim.user_chose.insert(address);
        }
    }

    /// One line per reason, written when the reason changes; `key` leaves out what changes every
    /// beat (the idle seconds).
    fn note_not_asking(&mut self, headset: &BluetoothHeadset, reason: &str, key: &str) {
        let key = format!("reclaim: {} not asked ({key})", headset.name);
        if self.reclaim.not_asking_logged.as_deref() == Some(key.as_str()) {
            return;
        }
        self.reclaim.not_asking_logged = Some(key);
        self.note(&format!("reclaim: {} not asked ({reason})", headset.name));
    }

    /// A headset that became the default output by itself ends its spell.
    fn forget_headsets_that_came_back(&mut self, snap: &DeviceSnapshot) {
        let back: Vec<String> = self
            .reclaim
            .names
            .iter()
            .filter(|(a, n)| reclaim::is_default_output(&headset(n, a), snap))
            .map(|(a, _)| a.clone())
            .collect();
        for address in back {
            self.end_spell(&address);
            self.reclaim.asked_this_playback.remove(&address);
            self.reclaim.user_chose.remove(&address);
        }
    }

    fn present_headsets(&self, addresses: &HashSet<String>, snap: &DeviceSnapshot) -> HashSet<String> {
        addresses
            .iter()
            .filter(|a| self.reclaim.names.get(*a).is_some_and(|n| reclaim::is_audio_device(&headset(n, a), snap)))
            .cloned()
            .collect()
    }

    fn held_down_headsets(&self) -> HashSet<String> {
        let now = self.now();
        self.reclaim.next_attempt.iter().filter(|(_, at)| now < **at).map(|(a, _)| a.clone()).collect()
    }

    /// Sends one request. The throttle is set as it goes out, so a lost reply cannot turn into a
    /// request per beat.
    pub(crate) fn request_route(&mut self, name: &str, address: &str, reason: &str) {
        let now = self.now();
        self.reclaim.next_attempt.insert(address.into(), now + RECLAIM_INTERVAL);
        self.reclaim.names.insert(address.into(), name.into());
        self.reclaim.returns.insert(address.into(), Return { name: name.into(), sent: now, accepted: false, arrived: None });
        self.routing.request(name, address, RECLAIM_SCORE, reason);
    }

    /// What the daemon said. Nothing here writes to CoreAudio.
    pub(crate) fn route_answered(&mut self, name: &str, address: &str, response: &RouteResponse) {
        self.routing.finish(address);
        let now = self.now();
        let outcome = response.outcome();
        if outcome != Outcome::Routed {
            self.reclaim.returns.remove(address);
        }
        match outcome {
            Outcome::Routed => {
                self.end_spell(address);
                self.reclaim.asked_this_playback.insert(address.into());
                self.note(&format!("reclaim: {name} <- remote device (hijack accepted)"));
                self.accept_return(address);
                for d in RETRY_BEATS_MS {
                    self.schedule_reconcile(d);
                }
            }
            Outcome::AlreadyRouted => {
                self.end_spell(address);
                self.reclaim.asked_this_playback.insert(address.into());
            }
            Outcome::HeldByRemote(detail) => {
                self.reclaim.next_attempt.insert(address.into(), now + RECLAIM_BACKOFF);
                let why = if detail.contains("Remote Category 501") {
                    ": the phone is on a call, Cleat yields by design"
                } else {
                    ""
                };
                self.note_held(&format!("reclaim: {name} held by remote device ({detail}){why}"), address);
            }
            Outcome::Busy => {
                self.reclaim.next_attempt.remove(address);
            }
            Outcome::Refused(detail) => {
                if short_lived(&detail) && self.retry_window_open(address) {
                    self.reclaim.next_attempt.insert(address.into(), now + RECLAIM_RETRY_DELAY);
                    self.schedule_reconcile(RECLAIM_RETRY_DELAY.as_millis() as u64);
                } else {
                    self.reclaim.next_attempt.insert(address.into(), now + RECLAIM_BACKOFF);
                }
                self.note_held(&format!("reclaim: {name} refused ({detail})"), address);
            }
        }
    }

    fn end_spell(&mut self, address: &str) {
        self.reclaim.held_logged.remove(address);
        self.reclaim.retry_window.remove(address);
    }

    fn retry_window_open(&mut self, address: &str) -> bool {
        let now = self.now();
        let start = *self.reclaim.retry_window.entry(address.into()).or_insert(now);
        now < start + RECLAIM_RETRY_SPAN
    }

    fn note_held(&mut self, message: &str, address: &str) {
        if self.reclaim.held_logged.get(address).map(String::as_str) == Some(message) {
            return;
        }
        self.reclaim.held_logged.insert(address.into(), message.into());
        self.note(message);
    }

    /// The hijack was accepted: log now if the headset already arrived, otherwise wait for it.
    fn accept_return(&mut self, address: &str) {
        let Some(r) = self.reclaim.returns.get_mut(address) else { return };
        r.accepted = true;
        if let Some(at) = r.arrived {
            let (name, secs) = (r.name.clone(), at.saturating_sub(r.sent).as_secs_f64());
            self.reclaim.returns.remove(address);
            self.note(&format!("reclaim: {name} back on the Mac in {secs:.1} s"));
        } else {
            let left = (r.sent + RETURN_TIMEOUT).saturating_sub(self.now());
            self.schedule_reconcile(left.as_millis() as u64);
        }
    }

    /// Called on every pass and on every device or default-output event, so the time is taken
    /// when the output moves rather than on the next scheduled beat.
    pub(crate) fn check_reclaim_returns(&mut self, snap: &DeviceSnapshot) {
        self.time_macos_return(snap);
        if self.reclaim.returns.is_empty() {
            return;
        }
        let now = self.now();
        let mut lines = vec![];
        self.reclaim.returns.retain(|address, r| {
            if r.arrived.is_none() && reclaim::is_default_output(&headset(&r.name, address), snap) {
                r.arrived = Some(now);
            }
            match (r.accepted, r.arrived) {
                (true, Some(at)) => {
                    lines.push(format!("reclaim: {} back on the Mac in {:.1} s", r.name, at.saturating_sub(r.sent).as_secs_f64()));
                    false
                }
                _ if now.saturating_sub(r.sent) >= RETURN_TIMEOUT => {
                    if r.accepted {
                        let where_ = if reclaim::is_audio_device(&headset(&r.name, address), snap) {
                            let output = snap.default_output.and_then(|id| snap.device(id)).map_or("nothing", |d| d.name.as_str());
                            format!("in CoreAudio, the output is {output}")
                        } else {
                            "not in CoreAudio".to_string()
                        };
                        lines.push(format!(
                            "reclaim: {} not back on the Mac {} s after the request ({where_})",
                            r.name,
                            RETURN_TIMEOUT.as_secs()
                        ));
                    }
                    false
                }
                _ => true,
            }
        });
        for line in lines {
            self.note(&line);
        }
    }

    /// macOS bringing a listed headset back by itself: timed from the first pass that saw the Mac
    /// playing with the headset elsewhere. Arrival is judged before the reset, so a playback that
    /// stops in the same move still counts. Cleat's own request or a hand-picked output voids it.
    fn time_macos_return(&mut self, snap: &DeviceSnapshot) {
        if self.config.reclaim_active().is_empty() {
            self.reclaim.macos_return.clear();
            return;
        }
        let now = self.now();
        let mut lines = vec![];
        let book = &mut self.reclaim;
        for (address, name) in &book.names {
            let voided = book.returns.contains_key(address) || book.user_chose.contains(address);
            if reclaim::is_default_output(&headset(name, address), snap) {
                let start = book.macos_return.remove(address).filter(|_| !voided);
                if let Some(took) = start.map(|s| now.saturating_sub(s)).filter(|t| *t <= MACOS_RETURN_LIMIT) {
                    let secs = took.as_secs_f64();
                    lines.push(format!("reclaim: {name} back on the Mac (macOS) in {secs:.1} s"));
                }
            } else if voided || !snap.output_running {
                book.macos_return.remove(address);
            } else {
                book.macos_return.entry(address.clone()).or_insert(now);
            }
        }
        for line in lines {
            self.note(&line);
        }
    }

    /// Remembers Cleat's own default-output write, so a pin moving the output is not the user's.
    pub(crate) fn note_own_output_write(&mut self, device: u32) {
        self.reclaim.own_output_write = Some((device, self.now()));
    }

    /// The `reclaim` line in `cleat status`.
    pub(crate) fn reclaim_summary(&self) -> String {
        let list = self.config.reclaim_active();
        if list.is_empty() {
            "off".into()
        } else if !self.routing.is_available() {
            format!("unavailable (no routing service on this macOS) ({})", list.join(", "))
        } else {
            format!("on ({})", list.join(", "))
        }
    }
}
