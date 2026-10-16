//! The remembered arrangements, `display-arrangements.json` (Swift `DisplayArrangementStore`).
//! Written atomically like the status file: a crash halfway through a write must not leave a
//! memory that reads as empty.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::rules::remember_displays::Origins;

/// Display set key (sorted UUIDs joined with `|`) to each display's top left, by UUID.
pub type Arrangements = BTreeMap<String, Origins>;

#[derive(Debug, PartialEq)]
pub enum Loaded {
    Empty,
    Loaded(Arrangements),
    Unreadable(String),
}

pub fn read(path: &Path) -> Loaded {
    if !path.exists() {
        return Loaded::Empty;
    }
    let decoded: Result<BTreeMap<String, BTreeMap<String, Vec<i32>>>, String> =
        std::fs::read(path).map_err(|e| e.to_string()).and_then(|d| serde_json::from_slice(&d).map_err(|e| e.to_string()));
    match decoded {
        Err(reason) => Loaded::Unreadable(reason),
        Ok(decoded) => {
            // A position is exactly x and y; anything else is a hand edit gone wrong, and the set
            // it belongs to is remembered afresh.
            let mut arrangements = Arrangements::new();
            for (key, places) in decoded {
                let valid: Origins =
                    places.into_iter().filter(|(_, v)| v.len() == 2).map(|(u, v)| (u, [v[0], v[1]])).collect();
                if !valid.is_empty() {
                    arrangements.insert(key, valid);
                }
            }
            Loaded::Loaded(arrangements)
        }
    }
}

/// False when the file could not be replaced.
pub fn write(arrangements: &Arrangements, path: &Path) -> bool {
    let Some(dir) = path.parent() else { return false };
    let _ = std::fs::create_dir_all(dir);
    let Ok(text) = serde_json::to_string_pretty(arrangements) else { return false };
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = dir.join(format!(".arrangements-{}-{nanos}.json", std::process::id()));
    if std::fs::write(&tmp, text).is_err() {
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}
