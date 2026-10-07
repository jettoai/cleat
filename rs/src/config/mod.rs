//! The declared state, as written in `~/.config/cleat/config.json`. Every field is optional and
//! falls back to a default that turns its rule off.

pub mod paths;
pub mod watcher;

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::{device_name, AudioDevice};

/// Per-device silence threshold for the liveness rule.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LivenessConfig {
    pub zero_seconds: f64,
}

/// A config file that parses but asks for something impossible.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    BalanceOutOfRange(f64),
    VolumeOutOfRange { device: String, value: f64 },
    ZeroSecondsTooSmall { device: String, value: f64 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::BalanceOutOfRange(v) => write!(f, "balance must be between 0 and 1, got {v:?}"),
            ConfigError::VolumeOutOfRange { device, value } => {
                write!(f, "inputVolume[\"{device}\"] must be between 0 and 100, got {value:?}")
            }
            ConfigError::ZeroSecondsTooSmall { device, value } => {
                write!(f, "liveness[\"{device}\"].zeroSeconds must be at least 1, got {value:?}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    Invalid(ConfigError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "{e}"),
            LoadError::Parse(e) => write!(f, "{e}"),
            LoadError::Invalid(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub input: Vec<String>,
    pub blocked_input: Vec<String>,
    pub output: Vec<String>,
    pub blocked_output: Vec<String>,
    pub headphones_take_over: bool,
    pub balance: Option<f64>,
    pub input_volume: BTreeMap<String, f64>,
    pub liveness: BTreeMap<String, LivenessConfig>,
    pub reclaim: Vec<String>,
    pub launch_at_login: bool,
    pub error_reports: bool,
    /// Rust only (§6.6): turns reclaim off without emptying `reclaim`. Read through `reclaim_active`.
    pub reclaim_enabled: bool,
    /// Writers whose output volume changes are reverted. Off unless asked for (Swift 0.3.10).
    pub output_volume_hold_against: Vec<String>,
    /// Rust only (§6.6): turns the hold off without emptying its list. Read through `hold_against_active`.
    pub output_volume_hold_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: vec![],
            blocked_input: vec![],
            output: vec![],
            blocked_output: vec![],
            headphones_take_over: false,
            balance: None,
            input_volume: BTreeMap::new(),
            liveness: BTreeMap::new(),
            reclaim: vec![],
            launch_at_login: true,
            error_reports: false,
            reclaim_enabled: true,
            output_volume_hold_against: vec![],
            output_volume_hold_enabled: true,
        }
    }
}

/// The `inputVolume` key that means "every input device present".
pub const INPUT_VOLUME_WILDCARD: &str = "*";

impl Config {
    /// What Cleat enforces when there is no config file at all: nothing.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// The reclaim list when the switch is on; the only reader of `reclaim_enabled`.
    pub fn reclaim_active(&self) -> &[String] {
        if self.reclaim_enabled { &self.reclaim } else { &[] }
    }

    /// The hold list when the switch is on; the only reader of `output_volume_hold_enabled`.
    pub fn hold_against_active(&self) -> &[String] {
        if self.output_volume_hold_enabled { &self.output_volume_hold_against } else { &[] }
    }

    /// Range checks the decoder cannot express.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if let Some(b) = self.balance {
            if !(0.0..=1.0).contains(&b) {
                return Err(ConfigError::BalanceOutOfRange(b));
            }
        }
        for (device, &value) in &self.input_volume {
            if !(0.0..=100.0).contains(&value) {
                return Err(ConfigError::VolumeOutOfRange { device: device.clone(), value });
            }
        }
        for (device, entry) in &self.liveness {
            if entry.zero_seconds < 1.0 {
                return Err(ConfigError::ZeroSecondsTooSmall { device: device.clone(), value: entry.zero_seconds });
            }
        }
        Ok(())
    }

    pub fn input_volume_has_wildcard(&self) -> bool {
        self.input_volume.contains_key(INPUT_VOLUME_WILDCARD)
    }

    /// The gain this device should be held at, in percent. A named entry (sorted order) wins over
    /// the wildcard.
    pub fn input_volume_target(&self, device: &AudioDevice) -> Option<f64> {
        self.input_volume
            .iter()
            .filter(|(k, _)| k.as_str() != INPUT_VOLUME_WILDCARD)
            .find(|(k, _)| device_name::matches(k, &device.name, &device.uid))
            .map(|(_, v)| *v)
            .or_else(|| self.input_volume.get(INPUT_VOLUME_WILDCARD).copied())
    }

    /// The devices among these whose gain this config asks to hold.
    pub fn input_volume_devices(&self, devices: &[AudioDevice]) -> Vec<AudioDevice> {
        if self.input_volume.is_empty() {
            return vec![];
        }
        let wildcard = self.input_volume_has_wildcard();
        devices
            .iter()
            .filter(|d| d.has_input && (wildcard || self.input_volume_target(d).is_some()))
            .cloned()
            .collect()
    }

    pub fn load(path: &Path) -> Result<Config, LoadError> {
        let data = std::fs::read(path).map_err(LoadError::Io)?;
        let config: Config = serde_json::from_slice(&data).map_err(LoadError::Parse)?;
        config.validate().map_err(LoadError::Invalid)?;
        Ok(config)
    }
}
