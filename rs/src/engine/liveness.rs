//! Silence detector bookkeeping and the microphone answer (Swift `Engine+Listeners.swift:137-209`,
//! `Engine.swift:153-162,300-309`). Log lines are word for word.

use std::collections::{HashMap, HashSet};

use super::{Engine, Mode};
use crate::model::{AudioDevice, DeviceSnapshot, Liveness, MicrophonePermission};

impl Engine {
    /// The dialog's answer, which arrives after the engine is running. An answer that changes
    /// nothing is not logged.
    pub fn update_microphone(&mut self, permission: MicrophonePermission) {
        if permission == self.microphone {
            return;
        }
        self.microphone = permission;
        self.note(&format!("microphone: {}", self.microphone.label()));
        self.rebind_devices();
        self.reconcile(false, None);
    }

    /// One detector per configured device that is plugged in, none for anything else. Safe to
    /// call on every event: a device with the right detector is left alone, so this is also the
    /// retry for a `start()` that failed. Observe mode opens nothing (D6).
    pub(crate) fn sync_liveness_detectors(&mut self, snap: &DeviceSnapshot) {
        if self.mode == Mode::Observe {
            return;
        }
        if !self.microphone.is_granted() {
            // Without the permission an IOProc reads silence forever, which looks exactly like a
            // transmitter that is off. Measure nothing instead.
            self.liveness_unavailable.clear();
            self.stop_all_liveness_detectors("no microphone permission");
            return;
        }

        let mut wanted: HashMap<String, (AudioDevice, f64)> = HashMap::new();
        for (entry, settings) in &self.config.liveness {
            if let Some(d) = snap.device_matching(entry, true) {
                wanted.insert(d.uid.clone(), (d.clone(), settings.zero_seconds));
            }
        }
        // A device no longer asked for starts its next spell with a clean slate.
        self.liveness_unavailable.retain(|uid| wanted.contains_key(uid));

        let mut running: Vec<String> = self.liveness_detectors.keys().cloned().collect();
        running.sort();
        for uid in running {
            let keep = match (wanted.get(&uid), self.liveness_detectors.get(&uid)) {
                (Some((device, zero)), Some(det)) => device.id == det.device_id() && *zero == det.zero_seconds(),
                _ => false,
            };
            if keep {
                continue;
            }
            if let Some(mut det) = self.liveness_detectors.remove(&uid) {
                det.stop();
                self.liveness_state.remove(&uid);
                self.note(&format!("liveness: {} -> stopped", det.name()));
            }
        }

        let mut uids: Vec<&String> = wanted.keys().collect();
        uids.sort();
        for uid in uids {
            if self.liveness_detectors.contains_key(uid) {
                continue;
            }
            let (device, zero_seconds) = &wanted[uid];
            let rate = self.system.nominal_sample_rate(device.id).unwrap_or(48_000.0);
            let mut det = (self.detectors)(device, rate, *zero_seconds);
            if det.start() {
                self.liveness_detectors.insert(uid.clone(), det);
                self.liveness_state.insert(uid.clone(), Liveness::Measuring);
                self.liveness_unavailable.remove(uid);
                self.note(&format!("liveness: {} -> measuring", device.name));
            } else if self.liveness_unavailable.insert(uid.clone()) {
                // Once per device, not once per beat.
                self.note(&format!("liveness: {} -> unavailable (could not open input)", device.name));
            }
        }
    }

    fn stop_all_liveness_detectors(&mut self, reason: &str) {
        let mut uids: Vec<String> = self.liveness_detectors.keys().cloned().collect();
        uids.sort();
        for uid in uids {
            if let Some(mut det) = self.liveness_detectors.remove(&uid) {
                det.stop();
                self.liveness_state.remove(&uid);
                self.note(&format!("liveness: {} -> stopped ({reason})", det.name()));
            }
        }
    }

    /// A verdict from a detector. A flip from one already stopped (still in the channel when it
    /// was replaced) is dropped.
    pub(crate) fn liveness_flipped(&mut self, uid: &str, name: &str, live: bool) {
        if !self.liveness_detectors.contains_key(uid) {
            return;
        }
        self.liveness_state.insert(uid.to_string(), if live { Liveness::Live } else { Liveness::Silent });
        self.note(&format!("liveness: {name} -> {}", if live { "live" } else { "silent" }));
        self.reconcile(false, None);
    }

    /// What the rules are told. While the dialog is unanswered, an unmeasured configured device
    /// reads as `measuring`, so nothing switches to it; a refusal leaves devices untracked.
    pub(crate) fn liveness_for_rules(&self, snap: &DeviceSnapshot) -> HashMap<String, Liveness> {
        let mut liveness = self.liveness_state.clone();
        if self.microphone != MicrophonePermission::Pending {
            return liveness;
        }
        for entry in self.config.liveness.keys() {
            if let Some(d) = snap.device_matching(entry, true) {
                liveness.entry(d.uid.clone()).or_insert(Liveness::Measuring);
            }
        }
        liveness
    }

    /// UIDs with a running detector, for tests.
    pub fn running_detector_uids(&self) -> HashSet<String> {
        self.liveness_detectors.keys().cloned().collect()
    }
}
