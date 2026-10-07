//! Posting an envelope, and the token that lets "off" stop a post already in flight.

use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::event::{ENVELOPE_URL, PUBLIC_KEY};

/// Whether reporting is on, and the post in flight. Turning off and registering a new post are
/// serialised on one lock, so a post either sees "off" when it registers, or is killed by it.
#[derive(Default)]
pub struct CancelToken {
    enabled: AtomicBool,
    in_flight: Mutex<Option<i32>>,
    changed: Condvar,
}

impl CancelToken {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// True when it was already on.
    pub(super) fn enable(&self) -> bool {
        self.enabled.swap(true, Ordering::SeqCst)
    }

    /// Off, and the process group of a post in flight is killed.
    pub(super) fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        let mut slot = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pid) = slot.take() {
            // SAFETY: plain signal; the child was started in its own process group.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
        }
        self.changed.notify_all();
    }

    /// Registers a started child. False means reporting is off: the caller must not send.
    pub fn register(&self, pid: i32) -> bool {
        let mut slot = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        if !self.is_enabled() {
            return false;
        }
        *slot = Some(pid);
        true
    }

    pub fn unregister(&self) {
        *self.in_flight.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Blocks until reporting is turned off or the timeout passes; true when off. For doubles
    /// that stand in for a post that never returns.
    pub fn wait_disabled(&self, timeout: Duration) -> bool {
        let slot = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        let (_slot, _) = self
            .changed
            .wait_timeout_while(slot, timeout, |_| self.is_enabled())
            .unwrap_or_else(|e| e.into_inner());
        !self.is_enabled()
    }
}

pub trait Transport: Send + Sync {
    /// The HTTP status, or why there is none.
    fn send(&self, envelope: &[u8], token: &CancelToken) -> Result<u16, String>;
}

/// `/usr/bin/curl` with the envelope on stdin, in its own process group so "off" can kill it.
pub struct Curl {
    pub client: String,
}

impl Transport for Curl {
    fn send(&self, envelope: &[u8], token: &CancelToken) -> Result<u16, String> {
        let auth = format!("X-Sentry-Auth: Sentry sentry_version=7, sentry_key={PUBLIC_KEY}, sentry_client={}", self.client);
        let mut child = Command::new("/usr/bin/curl")
            .args(["-sS", "--max-time", "15", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST", ENVELOPE_URL])
            .args(["-H", "Content-Type: application/x-sentry-envelope", "-H", &auth, "--data-binary", "@-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|e| e.to_string())?;
        let pid = child.id() as i32;
        if !token.register(pid) {
            // SAFETY: our own child's group.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
            let _ = child.wait();
            return Err("reporting turned off".into());
        }
        // curl reads all of stdin before connecting, so nothing leaves before this write ends.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(envelope);
        }
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut out);
        }
        let status = child.wait().map_err(|e| e.to_string());
        token.unregister();
        if !token.is_enabled() {
            return Err("reporting turned off".into());
        }
        let status = status?;
        if !status.success() {
            return Err(format!("curl exited {status}"));
        }
        out.trim().parse().map_err(|_| format!("no HTTP status in {out:?}"))
    }
}
