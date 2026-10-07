//! The append-only log behind `cleat log`. Rotates at a megabyte, keeping one older file. Only
//! actions that changed something are written (trace lines aside), as in Swift `EventLog.swift`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::clock::local_seconds;

const MAX_BYTES: u64 = 1_048_576;

pub struct EventLog {
    path: PathBuf,
    rotated: PathBuf,
}

impl EventLog {
    pub fn new(path: PathBuf, rotated: PathBuf) -> Self {
        Self { path, rotated }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `{local_seconds} {msg}` and returns the line. Failures are dropped: the log is a
    /// diagnostic.
    pub fn append(&self, msg: &str) -> String {
        let line = format!("{} {}", local_seconds(SystemTime::now()), msg);
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if fs::metadata(&self.path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
            let _ = fs::remove_file(&self.rotated);
            let _ = fs::rename(&self.path, &self.rotated);
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
        }
        line
    }

    /// Last `n` non-empty lines.
    pub fn tail(n: usize, path: &Path) -> Vec<String> {
        let Ok(text) = fs::read_to_string(path) else { return vec![] };
        let lines: Vec<String> = text.split('\n').filter(|l| !l.is_empty()).map(String::from).collect();
        lines[lines.len().saturating_sub(n)..].to_vec()
    }
}
