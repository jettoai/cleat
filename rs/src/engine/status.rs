//! What the daemon publishes after every reconcile, and the only thing `cleat status` reads.
//! Fields follow Swift `StatusStore.swift`; summaries follow the Swift engine word for word.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Engine;
use crate::config::{Config, INPUT_VOLUME_WILDCARD};
use crate::model::{AudioDevice, DeviceSnapshot, Liveness, MicrophonePermission};
use crate::state::clock::iso8601_utc;
use crate::state::reaction_clock::DaemonPerformance;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub pid: i32,
    pub updated_at: String,
    pub config_state: String,
    pub microphone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_reports: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_output: Option<String>,
    pub rules: BTreeMap<String, String>,
    pub liveness: BTreeMap<String, String>,
    pub recent_events: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_volume: Option<super::OutputVolumeStatus>,
    /// Reaction timing; None in a file written by a daemon older than the measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance: Option<DaemonPerformance>,
    /// The device each side is left on because it is "not used" and nothing else is usable
    /// (B-1287); None in a file written by a daemon older than the rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck: Option<StuckStatus>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StuckStatus {
    pub input: Option<String>,
    pub output: Option<String>,
}

impl Status {
    pub fn read(path: &Path) -> Option<Status> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Atomic: write a sibling temp file, then rename over the target.
    pub fn write(&self, path: &Path) {
        let Some(dir) = path.parent() else { return };
        let _ = std::fs::create_dir_all(dir);
        let Ok(value) = serde_json::to_value(self) else { return };
        let Ok(text) = serde_json::to_string_pretty(&value) else { return };
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let tmp = dir.join(format!(".status-{}-{nanos}.json", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

fn name_of(snap: &DeviceSnapshot, id: Option<u32>) -> Option<String> {
    id.and_then(|i| snap.device(i)).map(|d| d.name.clone())
}

impl Engine {
    pub(super) fn write_status(&self, snap: &DeviceSnapshot) {
        let stuck = StuckStatus { input: self.stuck.input.clone(), output: self.stuck.output.clone() };
        let status = Status {
            pid: std::process::id() as i32,
            updated_at: iso8601_utc(self.clock.wall()),
            config_state: self.config_state.clone(),
            microphone: self.microphone.label().into(),
            error_reports: Some(self.config.error_reports),
            default_input: name_of(snap, snap.default_input),
            default_output: name_of(snap, snap.default_output),
            rules: {
                let mut rules = rule_summaries(&self.config, snap, self.reclaim_summary(), &stuck);
                rules.insert("outputVolume".into(), self.output_volume_summary());
                rules
            },
            liveness: liveness_summaries(&self.config, snap, &self.microphone, &self.liveness_state),
            recent_events: self.recent_events.clone(),
            output_volume: Some(self.output_volume_status()),
            performance: Some(self.reactions.summary()),
            stuck: Some(stuck),
        };
        status.write(&self.status_path);
    }
}

pub fn rule_summaries(
    config: &Config,
    snap: &DeviceSnapshot,
    reclaim: String,
    stuck: &StuckStatus,
) -> BTreeMap<String, String> {
    let mut rules = BTreeMap::new();
    rules.insert("inputPin".into(), pin_summary(&config.input, &config.blocked_input, stuck.input.as_deref()));
    rules.insert("outputPin".into(), pin_summary(&config.output, &config.blocked_output, stuck.output.as_deref()));
    rules.insert("headphones".into(), headphones_summary(config));
    rules.insert("reclaim".into(), reclaim);
    rules.insert(
        "balance".into(),
        match config.balance {
            Some(b) => {
                let now = snap.output_balance.map_or("unreadable".into(), |v| format!("{v:.2}"));
                format!("on (target {b:.2}, now {now})")
            }
            None => "off".into(),
        },
    );
    let volume = if config.input_volume.is_empty() {
        "off".into()
    } else if let Some(&w) = config.input_volume.get(INPUT_VOLUME_WILDCARD) {
        wildcard_volume_summary(config, snap, w)
    } else {
        let parts: Vec<String> = config
            .input_volume
            .iter()
            .map(|(entry, wanted)| {
                match snap.device_matching(entry, true).and_then(|d| snap.input_volumes.get(&d.id)) {
                    Some(&cur) => format!("{entry} {wanted:.0}% (now {:.0}%)", cur as f64 * 100.0),
                    None => format!("{entry} {wanted:.0}% (absent)"),
                }
            })
            .collect();
        format!("on ({})", parts.join(", "))
    };
    rules.insert("inputVolume".into(), volume);
    rules
}

pub fn headphones_summary(config: &Config) -> String {
    if !config.headphones_take_over {
        return "off".into();
    }
    let mut s = "on (bluetooth output takes over when it connects".to_string();
    if !config.blocked_output.is_empty() {
        s += &format!("; blocked from taking over: {}", config.blocked_output.join(", "));
    }
    s + ")"
}

pub fn pin_summary(priority: &[String], blocked: &[String], stuck: Option<&str>) -> String {
    let mut s = if priority.is_empty() {
        if blocked.is_empty() {
            "off".into()
        } else {
            format!("on (no priority list, blocked: {})", blocked.join(", "))
        }
    } else {
        let mut s = format!("on ({})", priority.join(", "));
        if !blocked.is_empty() {
            s += &format!(", blocked: {}", blocked.join(", "));
        }
        s
    };
    if let Some(n) = stuck {
        s += &format!(", stuck on {n} (no other usable device)");
    }
    s
}

pub fn wildcard_volume_summary(config: &Config, snap: &DeviceSnapshot, wildcard: f64) -> String {
    let mut devices: Vec<&AudioDevice> = snap.devices.iter().filter(|d| d.has_input).collect();
    devices.sort_by(|a, b| AudioDevice::by_name(a, b));
    let mut parts: Vec<String> = devices
        .into_iter()
        .filter_map(|d| {
            let wanted = config.input_volume_target(d)?;
            Some(match snap.input_volumes.get(&d.id) {
                Some(&cur) => format!("{} {wanted:.0}% (now {:.0}%)", d.name, cur as f64 * 100.0),
                None => format!("{} {wanted:.0}% (unreadable)", d.name),
            })
        })
        .collect();
    parts.extend(
        config
            .input_volume
            .iter()
            .filter(|(k, _)| k.as_str() != INPUT_VOLUME_WILDCARD && snap.device_matching(k, true).is_none())
            .map(|(k, v)| format!("{k} {v:.0}% (absent)")),
    );
    if parts.is_empty() {
        format!("on (default {wildcard:.0}%)")
    } else {
        format!("on (default {wildcard:.0}%; {})", parts.join(", "))
    }
}

/// Swift `livenessSummaries`. A present, granted device with no verdict could not be opened.
pub fn liveness_summaries(
    config: &Config,
    snap: &DeviceSnapshot,
    microphone: &MicrophonePermission,
    state: &HashMap<String, Liveness>,
) -> BTreeMap<String, String> {
    config
        .liveness
        .keys()
        .map(|entry| {
            let Some(device) = snap.device_matching(entry, true) else {
                return (entry.clone(), "absent".to_string());
            };
            let v = if *microphone == MicrophonePermission::Pending {
                "awaiting microphone permission"
            } else if !microphone.is_granted() {
                "disabled (no microphone permission)"
            } else {
                match state.get(&device.uid) {
                    Some(Liveness::Measuring) => "measuring",
                    Some(Liveness::Live) => "live",
                    Some(Liveness::Silent) => "silent",
                    None => "unavailable",
                }
            };
            (entry.clone(), v.to_string())
        })
        .collect()
}
