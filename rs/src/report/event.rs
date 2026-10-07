//! Sentry events as JSON values, the home-folding scrub, and the envelope the transport posts.
//! Only two shapes are sent, a message and an exception; no user, no breadcrumbs.

use serde_json::{json, Value};

/// Sentry project DSN. The public key is not a secret: it ships in the Swift build's source too
/// (`ErrorReporting.swift:22-23`).
pub const DSN: &str = "https://5f28269bfd8a449ab060365e4870d043@o4508371539263488.ingest.us.sentry.io/4512198218088448";
pub const ENVELOPE_URL: &str = "https://o4508371539263488.ingest.us.sentry.io/api/4512198218088448/envelope/";
pub const PUBLIC_KEY: &str = "5f28269bfd8a449ab060365e4870d043";

/// What every event carries about the build and the machine.
#[derive(Debug, Clone, Default)]
pub struct Meta {
    /// `{bundle_id}@{version}+{build}`.
    pub release: String,
    /// `development` or `production`.
    pub environment: String,
    pub os_version: String,
}

/// 32 hex digits from the kernel's random source.
pub fn event_id() -> String {
    let mut bytes = [0u8; 16];
    // SAFETY: fills exactly the buffer it is given.
    unsafe { libc::arc4random_buf(bytes.as_mut_ptr().cast(), bytes.len()) };
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn base(meta: &Meta, level: &str, timestamp: f64) -> Value {
    json!({
        "event_id": event_id(),
        "timestamp": timestamp,
        "platform": "native",
        "level": level,
        "release": meta.release,
        "environment": meta.environment,
        "contexts": {"os": {"name": "macOS", "version": meta.os_version}},
    })
}

pub fn message(meta: &Meta, level: &str, timestamp: f64, text: &str) -> Value {
    let mut e = base(meta, level, timestamp);
    e["message"] = json!({"formatted": text});
    e
}

/// `frames` are Sentry frame objects, innermost last.
pub fn exception(meta: &Meta, timestamp: f64, kind: &str, value: &str, frames: Vec<Value>) -> Value {
    let mut e = base(meta, "fatal", timestamp);
    e["exception"] = json!({"values": [{"type": kind, "value": value, "stacktrace": {"frames": frames}}]});
    e
}

/// Folds the home directory to `~` in every string the event holds (message, exception value,
/// frame filename/module/package, debug_meta code_file, anything else) and drops the user
/// (Swift `ErrorReporting.scrub`).
pub fn scrub(event: &mut Value, home: &str) {
    if let Some(map) = event.as_object_mut() {
        map.remove("user");
    }
    fold(event, home);
}

fn fold(value: &mut Value, home: &str) {
    match value {
        Value::String(s) if !home.is_empty() && s.contains(home) => *s = s.replace(home, "~"),
        Value::Array(items) => items.iter_mut().for_each(|v| fold(v, home)),
        Value::Object(map) => map.values_mut().for_each(|v| fold(v, home)),
        _ => {}
    }
}

/// One-item envelope: header line, item header line, payload.
pub fn envelope(event: &Value, sent_at: &str) -> Vec<u8> {
    let payload = event.to_string();
    let header = json!({"event_id": event["event_id"], "dsn": DSN, "sent_at": sent_at});
    let item = json!({"type": "event", "length": payload.len()});
    format!("{header}\n{item}\n{payload}\n").into_bytes()
}
