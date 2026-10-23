//! The declared state, as written in `~/.config/cleat/config.json`. Every field is optional and
//! falls back to a default that turns its rule off.

pub mod paths;
pub mod watcher;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

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
    HeicLongEdgeOutOfRange(u32),
    HeicQualityOutOfRange(u32),
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
            ConfigError::HeicLongEdgeOutOfRange(v) => {
                write!(f, "heic.longEdge must be 0 or between 800 and 8000, got {v:?}")
            }
            ConfigError::HeicQualityOutOfRange(v) => write!(f, "heic.quality must be between 50 and 100, got {v:?}"),
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

/// Where HEIC photos are picked up and how they come out (`photos`). Paths are kept as written;
/// `~` is expanded where they are used.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HeicConfig {
    /// The folder watched, first level only.
    pub watch: String,
    /// A name (kept inside `watch`) or an absolute path the converted originals move to.
    pub originals: String,
    /// Pixels on the longer side; 0 keeps the original size.
    pub long_edge: u32,
    /// JPEG quality, 50 to 100.
    pub quality: u32,
}

impl Default for HeicConfig {
    fn default() -> Self {
        Self { watch: "~/Downloads".into(), originals: "HEIC原檔".into(), long_edge: 1200, quality: 60 }
    }
}

impl HeicConfig {
    pub fn watch_dir(&self, home: &Path) -> PathBuf {
        paths::expand_tilde(home, &self.watch)
    }

    pub fn originals_dir(&self, home: &Path) -> PathBuf {
        let dir = paths::expand_tilde(home, &self.originals);
        if dir.is_absolute() { dir } else { self.watch_dir(home).join(dir) }
    }
}

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
    /// The anonymous daily usage ping (`usage_ping`). On unless switched off.
    pub usage_ping: bool,
    /// Rust only (§6.6): `false` turns reclaim off without emptying `reclaim`; `true` keeps the
    /// settings window's switch on with an empty list (B-1287). Read through `reclaim_active`.
    pub reclaim_enabled: Option<bool>,
    /// Writers whose output volume changes are reverted. Off unless asked for (Swift 0.3.10).
    pub output_volume_hold_against: Vec<String>,
    /// Rust only (§6.6): turns the hold off without emptying its list. Read through `hold_against_active`.
    pub output_volume_hold_enabled: bool,
    /// Whether the last arrangement set for each combination of displays is remembered and put
    /// back when macOS rearranges it. Off unless asked for (Swift 0.4.2).
    pub remember_displays: bool,
    /// Converts HEIC files that arrive in `heic.watch` to JPEG. Off unless asked for.
    pub convert_heic: bool,
    pub heic: HeicConfig,
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
            usage_ping: true,
            reclaim_enabled: None,
            output_volume_hold_against: vec![],
            output_volume_hold_enabled: true,
            remember_displays: false,
            convert_heic: false,
            heic: HeicConfig::default(),
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

    /// The reclaim list when the switch is on; the daemon's only reader of `reclaim_enabled`.
    pub fn reclaim_active(&self) -> &[String] {
        if self.reclaim_enabled == Some(false) { &[] } else { &self.reclaim }
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
        let heic = &self.heic;
        if heic.long_edge != 0 && !(800..=8000).contains(&heic.long_edge) {
            return Err(ConfigError::HeicLongEdgeOutOfRange(heic.long_edge));
        }
        if !(50..=100).contains(&heic.quality) {
            return Err(ConfigError::HeicQualityOutOfRange(heic.quality));
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
