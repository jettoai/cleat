//! Opt-in error reporting to Sentry, without the SDK (plan D3, Swift `ErrorReporting.swift`).
//!
//! Events are spooled to files and posted by `/usr/bin/curl` on the `cleat-report` thread. Off
//! means off, including for what was queued before: turning off kills the post in flight, empties
//! the spool, and every post re-checks the switch when it registers, so nothing produced before
//! the switch went off can leave afterwards (the Swift build's known gap, done right here).

pub mod event;
pub mod ips;
pub mod spool;
pub mod transport;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

pub use event::Meta;
pub use transport::{CancelToken, Curl, Transport};

use crate::state::clock::iso8601_utc;

pub struct ReporterConfig {
    /// Spool and state live here.
    pub support_dir: PathBuf,
    /// Where macOS writes crash reports.
    pub diagnostics_dir: PathBuf,
    /// Folded to `~` in every event.
    pub home: String,
    pub meta: Meta,
    /// `CLEAT_SENTRY_TEST_EVENT=1`: send one probe message once reporting is on.
    pub probe: bool,
}

struct Inner {
    cfg: ReporterConfig,
    token: CancelToken,
    transport: Box<dyn Transport>,
    probe_sent: AtomicBool,
}

enum Job {
    Flush,
    Barrier(Sender<()>),
}

/// Cheap to clone; every clone drives the same worker.
#[derive(Clone)]
pub struct Reporter {
    inner: Arc<Inner>,
    tx: Sender<Job>,
}

impl Reporter {
    /// Starts off. The worker thread lives as long as any clone.
    pub fn new(cfg: ReporterConfig, transport: Box<dyn Transport>) -> Self {
        let inner = Arc::new(Inner { cfg, token: CancelToken::default(), transport, probe_sent: AtomicBool::new(false) });
        let (tx, rx) = channel::<Job>();
        let worker = inner.clone();
        let _ = std::thread::Builder::new().name("cleat-report".into()).spawn(move || {
            for job in rx {
                match job {
                    Job::Flush => worker.flush(),
                    Job::Barrier(done) => {
                        let _ = done.send(());
                    }
                }
            }
        });
        Self { inner, tx }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.token.is_enabled()
    }

    /// What the engine calls when `errorReports` changes.
    pub fn set_enabled(&self, on: bool) {
        let support = &self.inner.cfg.support_dir;
        if on {
            if self.inner.token.enable() {
                return;
            }
            // Crash reports count from when reporting was switched on; a restart while on keeps it.
            if !matches!(spool::read_state(support), Some((true, _))) {
                spool::write_state(support, true, spool::unix_now());
            }
            let _ = self.tx.send(Job::Flush);
        } else {
            self.inner.token.disable();
            spool::clear(support);
            spool::write_state(support, false, spool::unix_now());
        }
    }

    /// Spools one event and asks the worker to send it. Nothing at all while off.
    pub fn capture(&self, mut event: serde_json::Value) {
        if !self.is_enabled() {
            return;
        }
        event::scrub(&mut event, &self.inner.cfg.home);
        self.inner.spool(&event);
        let _ = self.tx.send(Job::Flush);
    }

    /// The panic hook's body: spooled synchronously, sent by the next process (`panic = "abort"`).
    pub fn record_panic(&self, message: &str) {
        self.inner.record_panic(message);
    }

    /// Returns once the worker has finished everything queued before this call. For tests.
    pub fn wait_idle(&self) {
        let (done, wait) = channel();
        if self.tx.send(Job::Barrier(done)).is_ok() {
            let _ = wait.recv();
        }
    }

    /// Installs the panic hook (daemon only), chained in front of the default one.
    pub fn install_panic_hook(&self) {
        let inner = self.inner.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            inner.record_panic(&info.to_string());
            previous(info);
        }));
    }
}

impl Inner {
    fn record_panic(&self, message: &str) {
        if !self.token.is_enabled() {
            return;
        }
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let mut e = event::exception(&self.cfg.meta, spool::unix_now(), "panic", message, vec![]);
        e["extra"] = serde_json::json!({"backtrace": trace});
        event::scrub(&mut e, &self.cfg.home);
        self.spool(&e);
    }

    /// Writes one event, and takes it back if reporting went off while it was being written:
    /// `set_enabled(false)` may have emptied the spool between the check and the write.
    fn spool(&self, event: &serde_json::Value) {
        if let Some(path) = spool::write(&self.cfg.support_dir, event) {
            if !self.token.is_enabled() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Crash reports since the mark, the probe if asked, then the spool, oldest first. Each
    /// step re-checks the switch.
    fn flush(&self) {
        if !self.token.is_enabled() {
            return;
        }
        self.collect_crash_reports();
        if self.cfg.probe && self.token.is_enabled() && !self.probe_sent.swap(true, Ordering::SeqCst) {
            let text = format!("cleat-rs sentry probe {}", iso8601_utc(std::time::SystemTime::now()));
            let mut e = event::message(&self.cfg.meta, "info", spool::unix_now(), &text);
            event::scrub(&mut e, &self.cfg.home);
            self.spool(&e);
        }
        for path in spool::list(&self.cfg.support_dir) {
            if !self.token.is_enabled() {
                return;
            }
            let Some(event) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()) else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            let envelope = event::envelope(&event, &iso8601_utc(std::time::SystemTime::now()));
            if let Ok(200..=299) = self.transport.send(&envelope, &self.token) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn collect_crash_reports(&self) {
        let support = &self.cfg.support_dir;
        let Some((true, since)) = spool::read_state(support) else { return };
        let mut mark = since;
        for (path, mtime) in ips::crash_files(&self.cfg.diagnostics_dir, since) {
            if !self.token.is_enabled() {
                return;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let (kind, value, frames) = ips::parse(&text, &name);
            let mut e = event::exception(&self.cfg.meta, mtime, &kind, &value, frames);
            event::scrub(&mut e, &self.cfg.home);
            self.spool(&e);
            mark = mark.max(mtime);
        }
        if mark > since && self.token.is_enabled() {
            spool::write_state(support, true, mark);
        }
    }
}

/// `kern.osproductversion`, e.g. "26.0".
pub fn os_version() -> String {
    let name = c"kern.osproductversion";
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: buf and len describe a writable buffer.
    let rc = unsafe { libc::sysctlbyname(name.as_ptr(), buf.as_mut_ptr().cast(), &mut len, std::ptr::null_mut(), 0) };
    if rc != 0 {
        return "unknown".into();
    }
    String::from_utf8_lossy(&buf[..len.min(buf.len())]).trim_end_matches('\0').to_string()
}
