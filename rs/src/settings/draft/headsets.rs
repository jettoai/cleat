//! The headphones page's half of the draft: the headset options, their ticks and the reclaim switch.

use crate::model::{AudioDevice, BluetoothHeadset};

use super::{HeadsetBox, HeadsetOption, PairedDevice, SettingsDraft};

/// Why reclaim, switched on, will do nothing (B-1287). The switch stays on; the page says why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimHint {
    NoHeadsets,
    AllHeadsetsBlocked,
    NoneTicked,
    AllTickedBlocked,
}

impl SettingsDraft {
    /// reclaim entries ∪ paired audio devices ∪ present Bluetooth outputs ∪ classless paired
    /// devices (`is_other`), each dropped when an earlier option names it.
    pub fn make_headset_options(reclaim: &[String], paired: &[PairedDevice], present: &[AudioDevice]) -> Vec<HeadsetOption> {
        let bluetooth: Vec<&AudioDevice> = present.iter().filter(|d| d.is_bluetooth() && d.has_output).collect();
        let mut options: Vec<HeadsetOption> = vec![];
        let entries = |o: &Vec<HeadsetOption>| o.iter().map(|x| x.entry.clone()).collect::<Vec<_>>();
        for entry in reclaim {
            if options.iter().any(|o| &o.entry == entry) {
                continue;
            }
            let one = [entry.clone()];
            let headset = paired.iter().find(|h| h.is_listed(&one));
            let device = bluetooth.iter().find(|d| d.is_listed(&one));
            options.push(HeadsetOption {
                entry: entry.clone(),
                display_name: headset.map(|h| h.name.clone()).or(device.map(|d| d.name.clone())).unwrap_or_else(|| entry.clone()),
                is_connected: headset.map_or(device.is_some(), |h| h.is_connected),
                is_selected: true,
                is_other: false,
            });
        }
        let mut sorted: Vec<&PairedDevice> = paired.iter().collect();
        sorted.sort_by(|a, b| b.is_connected.cmp(&a.is_connected).then_with(|| a.name.cmp(&b.name)));
        for h in &sorted {
            if h.is_audio() && !h.is_listed(&entries(&options)) {
                options.push(HeadsetOption { entry: h.name.clone(), display_name: h.name.clone(), is_connected: h.is_connected, is_selected: false, is_other: false });
            }
        }
        let mut bt = bluetooth.clone();
        bt.sort_by(|a, b| AudioDevice::by_name(a, b));
        for d in bt {
            if !d.is_listed(&entries(&options)) {
                options.push(HeadsetOption { entry: d.name.clone(), display_name: d.name.clone(), is_connected: true, is_selected: false, is_other: false });
            }
        }
        for h in &sorted {
            if h.minor_type.is_none() && !h.is_listed(&entries(&options)) {
                options.push(HeadsetOption { entry: h.name.clone(), display_name: h.name.clone(), is_connected: h.is_connected, is_selected: false, is_other: true });
            }
        }
        options
    }

    /// Turning reclaim on with nothing ticked ticks the first connected headset that is not "not
    /// used", else the first such. The switch goes on even with nothing to tick (B-1287): the page
    /// then says why nothing will happen, instead of the switch springing back off.
    pub fn set_reclaim_enabled(&mut self, on: bool) {
        if on && !self.headsets.iter().any(|h| h.is_selected) {
            // Classless devices (phones, computers) are never ticked on Albert's behalf.
            let open: Vec<usize> =
                (0..self.headsets.len()).filter(|&i| !self.headsets[i].is_other && !self.headset_blocked(&self.headsets[i])).collect();
            if let Some(i) = open.iter().copied().find(|&i| self.headsets[i].is_connected).or(open.first().copied()) {
                self.headsets[i].is_selected = true;
            }
        }
        self.reclaim_enabled = on;
    }

    /// The daemon never asks back a headset whose output is "not used" (`rules::reclaim::candidates`,
    /// same match), so the window shows it greyed out. Read from the current output stances, which
    /// can change while the window is open.
    pub fn headset_blocked(&self, h: &HeadsetOption) -> bool {
        let address = if h.entry == h.display_name { String::new() } else { BluetoothHeadset::canonical_address(&h.entry) };
        BluetoothHeadset { name: h.display_name.clone(), address, is_connected: h.is_connected }.is_listed(&self.output.blocked())
    }

    /// A blocked headset never shows a tick: ticked, it shows a dash and a click unticks it;
    /// unticked, it cannot be ticked.
    pub fn headset_box(&self, h: &HeadsetOption) -> HeadsetBox {
        match (self.headset_blocked(h), h.is_selected) {
            (false, on) => HeadsetBox::Plain(on),
            (true, true) => HeadsetBox::BlockedTicked,
            (true, false) => HeadsetBox::BlockedOff,
        }
    }

    /// Reclaim is on and every ticked headset's output is "not used": nothing will happen.
    pub fn all_ticked_blocked(&self) -> bool {
        let mut ticked = self.headsets.iter().filter(|h| h.is_selected).peekable();
        self.reclaim_enabled && ticked.peek().is_some() && ticked.all(|h| self.headset_blocked(h))
    }

    /// Why reclaim will do nothing while its switch is on; None while it is off or will act.
    pub fn reclaim_hint(&self) -> Option<ReclaimHint> {
        if !self.reclaim_enabled || self.headsets.iter().any(|h| h.is_selected && !self.headset_blocked(h)) {
            return None;
        }
        let audio: Vec<&HeadsetOption> = self.headsets.iter().filter(|h| !h.is_other).collect();
        Some(if self.headsets.is_empty() {
            ReclaimHint::NoHeadsets
        } else if !audio.is_empty() && audio.iter().all(|h| self.headset_blocked(h)) {
            ReclaimHint::AllHeadsetsBlocked
        } else if self.all_ticked_blocked() {
            ReclaimHint::AllTickedBlocked
        } else {
            ReclaimHint::NoneTicked
        })
    }

    /// Ignored while reclaim is off, and ticking a blocked headset is ignored. Unticking the last
    /// headset leaves the switch on (B-1287); the page then asks for a tick.
    pub fn set_headset(&mut self, entry: &str, on: bool) {
        if !self.reclaim_enabled {
            return;
        }
        let Some(i) = self.headsets.iter().position(|h| h.entry == entry) else { return };
        if on && self.headset_blocked(&self.headsets[i]) {
            return;
        }
        self.headsets[i].is_selected = on;
    }
}
