//! Writes the settings window's keys back into the config file, leaving every other key exactly as
//! it was (Swift `ConfigDocument.swift`). The window owns seven units, each a group of keys that
//! state one feature together (`UNITS`). A unit that still encodes as it did when the window
//! loaded is written back as the file had it then; a changed unit is written whole from the draft.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::config::Config;

/// The window's values, unit by unit. `reclaim` and `hold_against` are the lists, kept while their
/// switch is off; an empty `hold_against` writes no hold keys at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Managed {
    pub input: Vec<String>,
    pub blocked_input: Vec<String>,
    pub output: Vec<String>,
    pub blocked_output: Vec<String>,
    pub headphones_take_over: bool,
    pub input_volume: BTreeMap<String, f64>,
    pub reclaim: Vec<String>,
    pub reclaim_on: bool,
    pub balance: Option<f64>,
    pub hold_against: Vec<String>,
    pub hold_on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    InputStance,
    OutputStance,
    Takeover,
    Reclaim,
    Hold,
    InputVolume,
    Balance,
}

/// Every key the window owns, by the unit it belongs to. A unit is written whole or not at all.
pub const UNITS: [(Unit, &[&str]); 7] = [
    (Unit::InputStance, &["input", "blockedInput"]),
    (Unit::OutputStance, &["output", "blockedOutput"]),
    (Unit::Takeover, &["headphonesTakeOver"]),
    (Unit::Reclaim, &["reclaim", "reclaimEnabled"]),
    (Unit::Hold, &["outputVolumeHoldAgainst", "outputVolumeHoldEnabled"]),
    (Unit::InputVolume, &["inputVolume"]),
    (Unit::Balance, &["balance"]),
];

/// Canonical top-level order; keys outside it follow, sorted.
pub const KEY_ORDER: [&str; 13] = [
    "input",
    "blockedInput",
    "output",
    "blockedOutput",
    "headphonesTakeOver",
    "balance",
    "outputVolume",
    "outputVolumeHoldAgainst",
    "inputVolume",
    "liveness",
    "reclaim",
    "launchAtLogin",
    "errorReports",
];

fn number(v: f64) -> Value {
    if v == v.round() && v.abs() < 1e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

fn strings(list: &[String]) -> Value {
    Value::Array(list.iter().map(|s| Value::String(s.clone())).collect())
}

impl Managed {
    /// One unit's keys as the window writes them. A key of the unit missing here is removed.
    pub fn unit(&self, unit: Unit) -> Map<String, Value> {
        let mut m = Map::new();
        match unit {
            Unit::InputStance => {
                m.insert("input".into(), strings(&self.input));
                m.insert("blockedInput".into(), strings(&self.blocked_input));
            }
            Unit::OutputStance => {
                m.insert("output".into(), strings(&self.output));
                m.insert("blockedOutput".into(), strings(&self.blocked_output));
            }
            Unit::Takeover => {
                m.insert("headphonesTakeOver".into(), Value::Bool(self.headphones_take_over));
            }
            // Off with nothing ticked is Swift's `[]`; off with a list keeps it behind the Rust-only
            // `false`, so switching back on finds it.
            Unit::Reclaim => {
                m.insert("reclaim".into(), strings(&self.reclaim));
                if !self.reclaim_on && !self.reclaim.is_empty() {
                    m.insert("reclaimEnabled".into(), Value::Bool(false));
                }
            }
            Unit::Hold => {
                if !self.hold_against.is_empty() {
                    m.insert("outputVolumeHoldAgainst".into(), strings(&self.hold_against));
                    if !self.hold_on {
                        m.insert("outputVolumeHoldEnabled".into(), Value::Bool(false));
                    }
                }
            }
            Unit::InputVolume => {
                if !self.input_volume.is_empty() {
                    let obj = self.input_volume.iter().map(|(k, &v)| (k.clone(), number(v))).collect();
                    m.insert("inputVolume".into(), Value::Object(obj));
                }
            }
            Unit::Balance => {
                if let Some(b) = self.balance {
                    m.insert("balance".into(), number(b));
                }
            }
        }
        m
    }
}

/// What the window loaded: the file's top-level object then (empty when there was no file) and
/// what its draft encoded to then. Built once per load and never moved by a save: a unit switched
/// on, saved, then switched back off must come back as the file had it at load, not as last saved.
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    object: Map<String, Value>,
    managed: Managed,
}

impl Origin {
    pub fn new(raw: Option<&[u8]>, managed: Managed) -> Self {
        let object = match raw.map(serde_json::from_slice::<Value>) {
            Some(Ok(Value::Object(o))) => o,
            _ => Map::new(),
        };
        Self { object, managed }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriteError {
    NotAnObject,
    ChangedOnDisk,
    Invalid(String),
    Io(String),
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::NotAnObject => write!(f, "設定檔不是 JSON 物件，畫面不會覆寫它。"),
            WriteError::ChangedOnDisk => write!(f, "設定檔在畫面開著時被別處改過，請重新載入。"),
            WriteError::Invalid(r) => write!(f, "寫出的設定不合法：{r}"),
            WriteError::Io(r) => write!(f, "寫入失敗：{r}"),
        }
    }
}

/// Pure: existing bytes (None = no file) + every unit as the draft encodes it -> new bytes.
pub fn merged(raw: Option<&[u8]>, managed: &Managed) -> Result<Vec<u8>, WriteError> {
    merged_from(raw, managed, None)
}

/// Pure: as `merged`, except that a unit encoding as it did in `origin` is written as `origin`'s
/// file had it.
pub fn merged_from(raw: Option<&[u8]>, managed: &Managed, origin: Option<&Origin>) -> Result<Vec<u8>, WriteError> {
    let mut object = match raw {
        None => Map::new(),
        Some(bytes) => match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Object(o)) => o,
            _ => return Err(WriteError::NotAnObject),
        },
    };
    for (unit, keys) in UNITS {
        let now = managed.unit(unit);
        let from = match origin {
            Some(o) if o.managed.unit(unit) == now => &o.object,
            _ => &now,
        };
        for &key in keys {
            match from.get(key) {
                Some(v) => object.insert(key.into(), v.clone()),
                None => object.remove(key),
            };
        }
    }
    let known = KEY_ORDER.iter().filter(|k| object.contains_key(**k)).map(|k| k.to_string());
    let mut unknown: Vec<String> = object.keys().filter(|k| !KEY_ORDER.contains(&k.as_str())).cloned().collect();
    unknown.sort();
    let lines: Vec<String> = known
        .chain(unknown)
        .map(|k| format!("  {}: {}", Value::String(k.clone()), object[&k]))
        .collect();
    let out = format!("{{\n{}\n}}\n", lines.join(",\n")).into_bytes();
    let config: Config = serde_json::from_slice(&out).map_err(|e| WriteError::Invalid(e.to_string()))?;
    config.validate().map_err(|e| WriteError::Invalid(e.to_string()))?;
    Ok(out)
}

/// Refuses when the file no longer holds `baseline` (None = no file), then writes atomically.
/// Returns the bytes written, which become the next baseline; `origin` stays the one from load.
pub fn write(path: &Path, managed: &Managed, origin: Option<&Origin>, baseline: Option<&[u8]>) -> Result<Vec<u8>, WriteError> {
    let current = match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(WriteError::Io(e.to_string())),
    };
    if current.as_deref() != baseline {
        return Err(WriteError::ChangedOnDisk);
    }
    let out = merged_from(current.as_deref(), managed, origin)?;
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = target.parent().ok_or_else(|| WriteError::Io("no parent directory".into()))?;
    std::fs::create_dir_all(dir).map_err(|e| WriteError::Io(e.to_string()))?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = dir.join(format!(".config-{}-{nanos}.json", std::process::id()));
    std::fs::write(&tmp, &out).map_err(|e| WriteError::Io(e.to_string()))?;
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(WriteError::Io(e.to_string()));
    }
    Ok(out)
}
