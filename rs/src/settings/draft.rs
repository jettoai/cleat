//! The settings window's working copy (Swift `SettingsDraft.swift`, intents from
//! `SettingsStore.swift:119-176`). Pure: built from a config and what is present, turned back into
//! the keys the window owns by `managed()`.

use std::collections::{BTreeMap, HashSet};

use crate::config::{Config, INPUT_VOLUME_WILDCARD};
use crate::model::{device_name, AudioDevice, BluetoothHeadset};

use super::document::Managed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Input,
    Output,
}

/// Where a device stands: in the priority order, nowhere, or never to be used. One of the three,
/// so a device cannot be both preferred and not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    Listed,
    Neutral,
    Blocked,
}

/// One row of an input or output list. `entry` goes back into the file verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub entry: String,
    pub display_name: String,
    pub is_connected: bool,
    pub stance: Stance,
}

impl DeviceRow {
    pub fn is_listed(&self) -> bool {
        self.stance == Stance::Listed
    }

    pub fn is_blocked(&self) -> bool {
        self.stance == Stance::Blocked
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceList {
    pub rows: Vec<DeviceRow>,
}

impl DeviceList {
    /// Priority entries (config order), blocked-only entries (config order), then present devices
    /// no entry names (by name). Duplicate entries collapse to the first.
    /// An entry in both lists reads as blocked: "not used" outranks the order.
    pub fn make(priority: &[String], blocked: &[String], present: &[AudioDevice], input: bool) -> Self {
        let side: Vec<&AudioDevice> = present.iter().filter(|d| if input { d.has_input } else { d.has_output }).collect();
        let mut rows = vec![];
        let mut seen = HashSet::new();
        let entries: Vec<String> = priority.iter().chain(blocked).cloned().collect();
        for entry in &entries {
            if !seen.insert(entry.clone()) {
                continue;
            }
            let found = side.iter().find(|d| device_name::matches(entry, &d.name, &d.uid));
            rows.push(DeviceRow {
                entry: entry.clone(),
                display_name: found.map_or_else(|| entry.clone(), |d| d.name.clone()),
                is_connected: found.is_some(),
                stance: if blocked.contains(entry) {
                    Stance::Blocked
                } else if priority.contains(entry) {
                    Stance::Listed
                } else {
                    Stance::Neutral
                },
            });
        }
        let mut sorted = side.clone();
        sorted.sort_by(|a, b| AudioDevice::by_name(a, b));
        let mut seen_names = HashSet::new();
        for device in sorted {
            if device.is_listed(&entries) {
                continue;
            }
            if !seen_names.insert(device_name::normalize(&device.name)) || seen.contains(&device.name) {
                continue;
            }
            rows.push(DeviceRow {
                entry: device.name.clone(),
                display_name: device.name.clone(),
                is_connected: true,
                stance: Stance::Neutral,
            });
        }
        Self { rows }
    }

    pub fn listed(&self) -> Vec<DeviceRow> {
        self.rows.iter().filter(|r| r.is_listed()).cloned().collect()
    }

    pub fn others(&self) -> Vec<DeviceRow> {
        self.rows.iter().filter(|r| !r.is_listed()).cloned().collect()
    }

    pub fn priority(&self) -> Vec<String> {
        self.listed().into_iter().map(|r| r.entry).collect()
    }

    pub fn blocked(&self) -> Vec<String> {
        self.rows.iter().filter(|r| r.is_blocked()).map(|r| r.entry.clone()).collect()
    }

    /// Swift `Array.move(fromOffsets: [from], toOffset: to)`: `to` is the insertion point before
    /// removal, so up is `(i, i - 1)` and down is `(i, i + 2)`.
    pub fn move_listed(&mut self, from: usize, to: usize) {
        let mut listed = self.listed();
        if from >= listed.len() || to > listed.len() {
            return;
        }
        let row = listed.remove(from);
        let at = if to > from { to - 1 } else { to };
        listed.insert(at, row);
        self.rows = listed.into_iter().chain(self.others()).collect();
    }

    /// On (from any stance, lifting "not used") lands last of the listed rows; off (listed rows
    /// only) lands first of the others.
    pub fn set_listed(&mut self, entry: &str, on: bool) {
        let Some(index) = self.rows.iter().position(|r| r.entry == entry) else { return };
        if !on && !self.rows[index].is_listed() {
            return;
        }
        let mut row = self.rows.remove(index);
        row.stance = if on { Stance::Listed } else { Stance::Neutral };
        self.rows = self.listed().into_iter().chain([row]).chain(self.others()).collect();
    }

    /// On takes a listed row out of the order (first of the others); off makes a blocked row neutral.
    pub fn set_blocked(&mut self, entry: &str, on: bool) {
        let Some(index) = self.rows.iter().position(|r| r.entry == entry) else { return };
        match (on, self.rows[index].stance) {
            (true, Stance::Listed) => {
                let mut row = self.rows.remove(index);
                row.stance = Stance::Blocked;
                self.rows = self.listed().into_iter().chain([row]).chain(self.others()).collect();
            }
            (true, _) => self.rows[index].stance = Stance::Blocked,
            (false, Stance::Blocked) => self.rows[index].stance = Stance::Neutral,
            (false, _) => {}
        }
    }
}

/// One paired Bluetooth device, with the class `system_profiler` reports (Swift `BluetoothHeadset`).
#[derive(Debug, Clone, PartialEq)]
pub struct PairedDevice {
    pub name: String,
    pub address: String,
    pub is_connected: bool,
    pub minor_type: Option<String>,
}

impl PairedDevice {
    /// Swift `BluetoothHeadsets.swift:20-23`.
    pub fn is_audio(&self) -> bool {
        let Some(kind) = &self.minor_type else { return false };
        let lower = kind.to_lowercase();
        ["head", "speaker", "audio", "hands-free", "earbud"].iter().any(|w| lower.contains(w))
    }

    pub fn is_listed(&self, entries: &[String]) -> bool {
        BluetoothHeadset { name: self.name.clone(), address: self.address.clone(), is_connected: self.is_connected }
            .is_listed(entries)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadsetOption {
    pub entry: String,
    pub display_name: String,
    pub is_connected: bool,
    pub is_selected: bool,
    pub is_other: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeEntry {
    pub entry: String,
    pub display_name: String,
    pub is_connected: bool,
    pub percent: f64,
}

/// What the default devices are at right now.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveLevels {
    pub output_device: Option<String>,
    pub output_volume: Option<f64>,
    pub balance: Option<f64>,
    pub input_device: Option<String>,
    pub input_volume: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDraft {
    pub output: DeviceList,
    pub input: DeviceList,
    pub headphones_take_over: bool,
    pub reclaim_enabled: bool,
    pub headsets: Vec<HeadsetOption>,
    pub wildcard_enabled: bool,
    pub wildcard_percent: f64,
    pub named_volumes: Vec<VolumeEntry>,
    pub balance_enabled: bool,
    pub balance: f64,
    pub hold_enabled: bool,
    pub hold_against: Vec<String>,
}

pub const DEFAULT_HOLD_AGAINST: &str = "Parallels Desktop";

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

impl SettingsDraft {
    /// Unset values start from what the device is at now, so switching "fixed" on holds the
    /// current level. The two Rust-only switches (§2.5 of the plan) count as off when false.
    pub fn make(config: &Config, present: &[AudioDevice], paired: &[PairedDevice], live: &LiveLevels) -> Self {
        let inputs: Vec<&AudioDevice> = present.iter().filter(|d| d.has_input).collect();
        let wildcard = config.input_volume.get(INPUT_VOLUME_WILDCARD).copied();
        let named = config
            .input_volume
            .iter()
            .filter(|(k, _)| k.as_str() != INPUT_VOLUME_WILDCARD)
            .map(|(k, &v)| {
                let found = inputs.iter().find(|d| device_name::matches(k, &d.name, &d.uid));
                VolumeEntry {
                    entry: k.clone(),
                    display_name: found.map_or_else(|| k.clone(), |d| d.name.clone()),
                    is_connected: found.is_some(),
                    percent: v,
                }
            })
            .collect();
        let hold = &config.output_volume_hold_against;
        Self {
            output: DeviceList::make(&config.output, &config.blocked_output, present, false),
            input: DeviceList::make(&config.input, &config.blocked_input, present, true),
            headphones_take_over: config.headphones_take_over,
            reclaim_enabled: config.reclaim_enabled && !config.reclaim.is_empty(),
            headsets: Self::make_headset_options(&config.reclaim, paired, present),
            wildcard_enabled: wildcard.is_some(),
            wildcard_percent: wildcard.or(live.input_volume.map(f64::round)).unwrap_or(100.0),
            named_volumes: named,
            balance_enabled: config.balance.is_some(),
            balance: config.balance.or(live.balance.map(round2)).unwrap_or(0.5),
            hold_enabled: config.output_volume_hold_enabled && !hold.is_empty(),
            hold_against: if hold.is_empty() { vec![DEFAULT_HOLD_AGAINST.into()] } else { hold.clone() },
        }
    }

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

    pub fn managed(&self) -> Managed {
        let mut volumes = BTreeMap::new();
        for e in &self.named_volumes {
            volumes.insert(e.entry.clone(), e.percent.round());
        }
        if self.wildcard_enabled {
            volumes.insert(INPUT_VOLUME_WILDCARD.to_string(), self.wildcard_percent.round());
        }
        // An off hold showing only the placeholder is what a file with no hold keys reads as.
        let placeholder = !self.hold_enabled && self.hold_against.len() == 1 && self.hold_against[0] == DEFAULT_HOLD_AGAINST;
        Managed {
            input: self.input.priority(),
            blocked_input: self.input.blocked(),
            output: self.output.priority(),
            blocked_output: self.output.blocked(),
            headphones_take_over: self.headphones_take_over,
            input_volume: volumes,
            reclaim: self.headsets.iter().filter(|h| h.is_selected).map(|h| h.entry.clone()).collect(),
            reclaim_on: self.reclaim_enabled,
            balance: self.balance_enabled.then(|| round2(self.balance)),
            hold_against: if placeholder { vec![] } else { self.hold_against.clone() },
            hold_on: self.hold_enabled,
        }
    }

    pub fn list_mut(&mut self, side: Side) -> &mut DeviceList {
        match side {
            Side::Input => &mut self.input,
            Side::Output => &mut self.output,
        }
    }

    pub fn set_hold_enabled(&mut self, on: bool) {
        self.hold_enabled = on;
        if on && self.hold_against.is_empty() {
            self.hold_against = vec![DEFAULT_HOLD_AGAINST.into()];
        }
    }

    pub fn add_hold_app(&mut self, name: &str) {
        if !name.is_empty() && !self.hold_against.iter().any(|n| n == name) {
            self.hold_against.push(name.into());
        }
    }

    /// Removing the last entry turns the hold off.
    pub fn remove_hold_app(&mut self, name: &str) {
        self.hold_against.retain(|n| n != name);
        if self.hold_against.is_empty() {
            self.hold_enabled = false;
        }
    }

    /// Turning reclaim on with nothing ticked ticks the first connected headset, else the first;
    /// with no headset at all it stays off.
    pub fn set_reclaim_enabled(&mut self, on: bool) {
        if on && !self.headsets.iter().any(|h| h.is_selected) {
            let index = self.headsets.iter().position(|h| h.is_connected).or((!self.headsets.is_empty()).then_some(0));
            let Some(i) = index else { return };
            self.headsets[i].is_selected = true;
        }
        self.reclaim_enabled = on;
    }

    /// Ignored while reclaim is off. Unticking the last headset turns reclaim off.
    pub fn set_headset(&mut self, entry: &str, on: bool) {
        if !self.reclaim_enabled {
            return;
        }
        if let Some(h) = self.headsets.iter_mut().find(|h| h.entry == entry) {
            h.is_selected = on;
        }
        if !self.headsets.iter().any(|h| h.is_selected) {
            self.reclaim_enabled = false;
        }
    }

    pub fn add_volume(&mut self, name: &str, present: &[AudioDevice]) {
        if self.named_volumes.iter().any(|v| v.entry == name) {
            return;
        }
        self.named_volumes.push(VolumeEntry {
            entry: name.into(),
            display_name: name.into(),
            is_connected: present.iter().any(|d| d.name == name),
            percent: if self.wildcard_enabled { self.wildcard_percent } else { 100.0 },
        });
        self.named_volumes.sort_by(|a, b| a.entry.cmp(&b.entry));
    }

    pub fn remove_volume(&mut self, entry: &str) {
        self.named_volumes.retain(|v| v.entry != entry);
    }

    /// Present input devices no named volume entry matches yet, by raw name.
    pub fn volume_candidates(&self, present: &[AudioDevice]) -> Vec<String> {
        let entries: Vec<String> = self.named_volumes.iter().map(|v| v.entry.clone()).collect();
        let mut inputs: Vec<&AudioDevice> = present.iter().filter(|d| d.has_input).collect();
        inputs.sort_by(|a, b| AudioDevice::by_name(a, b));
        let mut names: Vec<String> = vec![];
        for d in inputs {
            if !d.is_listed(&entries) && !names.contains(&d.name) {
                names.push(d.name.clone());
            }
        }
        names
    }
}
