//! Writes the settings window's keys back into the config file, leaving every other key exactly as
//! it was (Swift `ConfigDocument.swift`). The Rust-only switches `reclaimEnabled` and
//! `outputVolumeHoldEnabled` are not owned: the daemon reads them, so they are carried verbatim,
//! except that a `false` one is removed (default true) when the window turns its feature on.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::config::Config;

/// The keys the window owns. `balance` and `hold_against` are None when not held: the key is then
/// removed from the file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Managed {
    pub input: Vec<String>,
    pub blocked_input: Vec<String>,
    pub output: Vec<String>,
    pub blocked_output: Vec<String>,
    pub headphones_take_over: bool,
    pub input_volume: BTreeMap<String, f64>,
    pub reclaim: Vec<String>,
    pub balance: Option<f64>,
    pub hold_against: Option<Vec<String>>,
    /// Owned keys left exactly as the file has them (a list parked behind a Rust-only `false`).
    pub carried: Vec<&'static str>,
}

/// Swift's nine keys.
pub const MANAGED_KEYS: [&str; 9] = [
    "input",
    "blockedInput",
    "output",
    "blockedOutput",
    "headphonesTakeOver",
    "inputVolume",
    "reclaim",
    "balance",
    "outputVolumeHoldAgainst",
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
    /// A key missing here is removed from the file.
    pub fn values(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("input".into(), strings(&self.input));
        m.insert("blockedInput".into(), strings(&self.blocked_input));
        m.insert("output".into(), strings(&self.output));
        m.insert("blockedOutput".into(), strings(&self.blocked_output));
        m.insert("headphonesTakeOver".into(), Value::Bool(self.headphones_take_over));
        m.insert("reclaim".into(), strings(&self.reclaim));
        if !self.input_volume.is_empty() {
            let obj = self.input_volume.iter().map(|(k, &v)| (k.clone(), number(v))).collect();
            m.insert("inputVolume".into(), Value::Object(obj));
        }
        if let Some(b) = self.balance {
            m.insert("balance".into(), number(b));
        }
        if let Some(list) = &self.hold_against {
            m.insert("outputVolumeHoldAgainst".into(), strings(list));
        }
        m
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

/// Pure: existing bytes (None = no file) + managed values -> new bytes.
pub fn merged(raw: Option<&[u8]>, managed: &Managed) -> Result<Vec<u8>, WriteError> {
    let mut object = match raw {
        None => Map::new(),
        Some(bytes) => match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Object(o)) => o,
            _ => return Err(WriteError::NotAnObject),
        },
    };
    let values = managed.values();
    for key in MANAGED_KEYS.into_iter().filter(|k| !managed.carried.contains(k)) {
        match values.get(key) {
            Some(v) => object.insert(key.into(), v.clone()),
            None => object.remove(key),
        };
    }
    for (on, key) in [(!managed.reclaim.is_empty(), "reclaimEnabled"), (managed.hold_against.is_some(), "outputVolumeHoldEnabled")] {
        if on && object.get(key) == Some(&Value::Bool(false)) {
            object.remove(key);
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
/// Returns the bytes written, which become the next baseline.
pub fn write(path: &Path, managed: &Managed, baseline: Option<&[u8]>) -> Result<Vec<u8>, WriteError> {
    let current = match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(WriteError::Io(e.to_string())),
    };
    if current.as_deref() != baseline {
        return Err(WriteError::ChangedOnDisk);
    }
    let out = merged(current.as_deref(), managed)?;
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
