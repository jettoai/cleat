//! The anonymous install count: one POST to jetto.ai when the daemon starts and then at most once
//! a day while it runs, carrying the product, a random device id and the Cleat version. Nothing
//! else: no OS version, no device names, no config values, no Authorization header. Failures are
//! dropped, never retried, and never waited on by the engine. On by default; `"usagePing": false`
//! in config.json turns it off, and off means no request at all and no device id file.
//!
//! Daemon only: the CLI half of the binary returns from `main` before anything here can run.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime};

pub const PRODUCT: &str = "cleat";
pub const ENDPOINT: &str = "https://jetto.ai/api/v1/ping";
pub const DEVICE_FILE_NAME: &str = "usage-ping-device";
/// Dev builds only: `CLEAT_PING_URL=http://127.0.0.1:<port>/api/v1/ping` points the ping at a local
/// stand-in. Without it a development build sends nothing, so development runs do not count as installs.
pub const URL_OVERRIDE_VARIABLE: &str = "CLEAT_PING_URL";
pub const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);

fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn new_uuid() -> Option<String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").ok()?.read_exact(&mut b).ok()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Some(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
}

/// The stored id, made on first use. A file that does not hold a UUID is replaced. None when the
/// id cannot be kept: a fresh id on every ping would count one Mac many times, so no ping goes.
pub fn device_id(path: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(path) {
        let stored = text.trim();
        if is_uuid(stored) {
            return Some(stored.to_lowercase());
        }
    }
    let id = new_uuid()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::write(path, &id).ok()?;
    Some(id)
}

/// The JSON body, keys sorted, byte for byte what the Swift build sends.
pub fn body(device: &str, version: &str) -> String {
    let fields = std::collections::BTreeMap::from([("device", device), ("product", PRODUCT), ("version", version)]);
    serde_json::to_string(&fields).unwrap_or_default()
}

pub trait PingTransport: Send + Sync {
    /// One POST of `body` as JSON. Failure is the transport's to drop.
    fn post(&self, url: &str, body: &str);
}

/// `/usr/bin/curl` with the body on stdin: no Authorization header, 20 seconds at most.
pub struct CurlPost;

impl CurlPost {
    pub fn args(url: &str) -> Vec<String> {
        // `-q` has to come first: it keeps a ~/.curlrc (a `header = "Authorization: ..."` line)
        // from adding what this request promises not to carry.
        ["-q", "-sS", "--max-time", "20", "-o", "/dev/null", "-X", "POST", url, "-H", "Content-Type: application/json", "--data-binary", "@-"]
            .map(String::from)
            .to_vec()
    }
}

impl CurlPost {
    /// The curl process `post` runs, so a test can run the same one with its own environment.
    pub fn command(url: &str) -> Command {
        let mut c = Command::new("/usr/bin/curl");
        c.args(Self::args(url)).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    }
}

impl PingTransport for CurlPost {
    fn post(&self, url: &str, body: &str) {
        let Ok(mut child) = Self::command(url).spawn() else { return };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Sends one ping if `enabled`; returns whether a request went out. With `enabled` false it
/// touches nothing: no id file, no request.
pub fn send(enabled: bool, url: &str, device_file: &Path, version: &str, transport: &dyn PingTransport) -> bool {
    let Some(device) = enabled.then(|| device_id(device_file)).flatten() else { return false };
    transport.post(url, &body(&device, version));
    true
}

/// Whether a timer tick should send: a day since the last ping, by the wall clock. A tick after
/// a long sleep sees the gap once and sends once, then the stamp moves.
pub fn is_due(last_sent: Option<SystemTime>, now: SystemTime) -> bool {
    // A clock set back reads as not yet due, like Swift's negative interval.
    last_sent.is_none_or(|t| now.duration_since(t).is_ok_and(|d| d >= INTERVAL))
}

/// A release build always uses the real endpoint and never reads `override_`; a development build
/// sends only when `override_` names a stand-in.
pub fn resolved_url(development: bool, override_: Option<&str>) -> Option<String> {
    if !development {
        return Some(ENDPOINT.into());
    }
    override_.filter(|u| !u.is_empty()).map(String::from)
}

/// Owns the hourly check. `apply(true)` pings now and then once a day; `apply(false)` stops it.
pub struct Pinger {
    url: Option<String>,
    device_file: std::path::PathBuf,
    version: String,
    transport: std::sync::Arc<dyn PingTransport>,
    stop: Option<Sender<()>>,
}

impl Pinger {
    pub fn new(url: Option<String>, device_file: std::path::PathBuf, version: String, transport: std::sync::Arc<dyn PingTransport>) -> Self {
        Self { url, device_file, version, transport, stop: None }
    }

    pub fn apply(&mut self, on: bool) {
        if !on {
            self.stop = None; // the worker sees the sender go and ends
            return;
        }
        let (Some(url), None) = (self.url.clone(), &self.stop) else { return };
        let (tx, rx) = mpsc::channel::<()>();
        self.stop = Some(tx);
        let (file, version, transport) = (self.device_file.clone(), self.version.clone(), self.transport.clone());
        let worker = std::thread::Builder::new().name("cleat-usage-ping".into()).spawn(move || {
            let mut last_sent = None;
            loop {
                let now = SystemTime::now();
                if is_due(last_sent, now) && send(true, &url, &file, &version, transport.as_ref()) {
                    last_sent = Some(now);
                }
                // The wall clock decides, not this sleep: a Mac asleep does not count against it.
                if !matches!(rx.recv_timeout(CHECK_EVERY), Err(RecvTimeoutError::Timeout)) {
                    return;
                }
            }
        });
        if worker.is_err() {
            self.stop = None;
        }
    }
}
