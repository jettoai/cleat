//! Native crashes, read back from the `.ips` files macOS writes to
//! `~/Library/Logs/DiagnosticReports`: a JSON header line, then a JSON body.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{json, Value};

/// `Cleat-rs*.ips` files modified after `since` (Unix seconds), oldest first, with their mtimes.
pub fn crash_files(dir: &Path, since: f64) -> Vec<(PathBuf, f64)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut found: Vec<(PathBuf, f64)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("Cleat-rs") || !name.ends_with(".ips") {
                return None;
            }
            let mtime = e.metadata().ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs_f64();
            (mtime > since).then_some((e.path(), mtime))
        })
        .collect();
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    found
}

/// (exception type, value, frames) from a crash report. A body that does not parse still
/// yields a one-line description with the file name, so a crash is never dropped.
pub fn parse(text: &str, file_name: &str) -> (String, String, Vec<Value>) {
    let fallback = || ("crash".to_string(), format!("crash report {file_name} (unreadable)"), vec![]);
    let Some((_header, body)) = text.split_once('\n') else { return fallback() };
    let Ok(body) = serde_json::from_str::<Value>(body) else { return fallback() };
    let exc = &body["exception"];
    let kind = exc["type"].as_str().unwrap_or("crash").to_string();
    let mut value = [exc["signal"].as_str(), exc["subtype"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(" ");
    if let Some(ind) = body["termination"]["indicator"].as_str() {
        value = if value.is_empty() { ind.to_string() } else { format!("{value} ({ind})") };
    }
    let images = body["usedImages"].as_array().cloned().unwrap_or_default();
    let thread = body["faultingThread"].as_u64().and_then(|i| body["threads"].get(i as usize));
    let mut frames: Vec<Value> = thread
        .and_then(|t| t["frames"].as_array())
        .map(|fs| {
            fs.iter()
                .map(|f| {
                    let image = f["imageIndex"].as_u64().and_then(|i| images.get(i as usize));
                    // The offset rides in the function name: it is image-relative, not an address.
                    json!({
                        "function": format!("{} +0x{:x}", f["symbol"].as_str().unwrap_or("?"), f["imageOffset"].as_u64().unwrap_or(0)),
                        "package": image.and_then(|im| im["path"].as_str().or(im["name"].as_str())).unwrap_or("?"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Sentry wants the innermost frame last; .ips lists it first.
    frames.reverse();
    (kind, if value.is_empty() { file_name.to_string() } else { value }, frames)
}
