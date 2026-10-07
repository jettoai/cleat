//! Events waiting to be sent, one JSON file each under `<support>/reports/`, and the persisted
//! "on since" mark that decides which crash reports are ours to send.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// Older events beyond this are dropped when a new one is written.
pub const KEEP: usize = 20;

pub fn dir(support: &Path) -> PathBuf {
    support.join("reports")
}

fn state_path(support: &Path) -> PathBuf {
    support.join("reports-state.json")
}

/// Atomic write (temp file, then rename), then the oldest beyond `KEEP` go.
pub fn write(support: &Path, event: &Value) -> Option<PathBuf> {
    let dir = dir(support);
    std::fs::create_dir_all(&dir).ok()?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let id = event["event_id"].as_str().unwrap_or("event");
    let path = dir.join(format!("{nanos:020}-{id}.json"));
    let tmp = dir.join(format!(".{nanos}-{id}.tmp"));
    std::fs::write(&tmp, event.to_string()).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    let files = list(support);
    for old in files.iter().take(files.len().saturating_sub(KEEP)) {
        let _ = std::fs::remove_file(old);
    }
    Some(path)
}

/// Spooled events, oldest first (names start with a zero-padded timestamp).
pub fn list(support: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir(support)) else { return vec![] };
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    files
}

/// Every spooled event, gone.
pub fn clear(support: &Path) {
    let Ok(entries) = std::fs::read_dir(dir(support)) else { return };
    for e in entries.flatten() {
        let _ = std::fs::remove_file(e.path());
    }
}

/// Whether reporting was on when last written, and since when (Unix seconds).
pub fn read_state(support: &Path) -> Option<(bool, f64)> {
    let v: Value = serde_json::from_slice(&std::fs::read(state_path(support)).ok()?).ok()?;
    Some((v["on"].as_bool()?, v["since"].as_f64()?))
}

pub fn write_state(support: &Path, on: bool, since: f64) {
    let _ = std::fs::create_dir_all(support);
    let _ = std::fs::write(state_path(support), serde_json::json!({"on": on, "since": since}).to_string());
}

pub fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}
