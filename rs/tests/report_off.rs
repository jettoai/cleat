//! "Off means off" (plan §8 #17), the case the Swift build gets wrong: nothing produced before
//! reporting is switched off may be sent after it, with a transport double that never returns.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use cleat_rs::report::{event, spool, CancelToken, Meta, Reporter, ReporterConfig, Transport};

#[derive(Default)]
struct Counts {
    attempts: AtomicUsize,
    sent: AtomicUsize,
}

/// `Block`: every post hangs until reporting goes off (a post in flight). `Accept`: 200.
#[derive(Clone, Copy, PartialEq)]
enum Behaviour {
    Block,
    Accept,
}

struct Fake {
    counts: Arc<Counts>,
    behaviour: Behaviour,
    entered: Mutex<Option<Sender<()>>>,
}

impl Transport for Fake {
    fn send(&self, _envelope: &[u8], token: &CancelToken) -> Result<u16, String> {
        self.counts.attempts.fetch_add(1, Ordering::SeqCst);
        if let Some(tx) = self.entered.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
        if self.behaviour == Behaviour::Block && token.wait_disabled(Duration::from_secs(10)) {
            return Err("cancelled".into());
        }
        self.counts.sent.fetch_add(1, Ordering::SeqCst);
        Ok(200)
    }
}

struct Setup {
    dir: PathBuf,
    diag: PathBuf,
    counts: Arc<Counts>,
    reporter: Reporter,
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

static N: AtomicUsize = AtomicUsize::new(0);

fn setup(behaviour: Behaviour, probe: bool, entered: Option<Sender<()>>) -> Setup {
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("cleat-rs-report-{}-{n}", std::process::id()));
    let diag = dir.join("DiagnosticReports");
    std::fs::create_dir_all(&diag).unwrap();
    let counts = Arc::new(Counts::default());
    let cfg = ReporterConfig {
        support_dir: dir.join("support"),
        diagnostics_dir: diag.clone(),
        home: "/Users/someone".into(),
        meta: Meta::default(),
        probe,
    };
    let fake = Fake { counts: counts.clone(), behaviour, entered: Mutex::new(entered) };
    let reporter = Reporter::new(cfg, Box::new(fake));
    Setup { dir, diag, counts, reporter }
}

fn spooled(s: &Setup) -> usize {
    spool::list(&s.dir.join("support")).len()
}

fn msg(text: &str) -> serde_json::Value {
    event::message(&Meta::default(), "error", 0.0, text)
}

#[test]
fn a_switch_off_cancels_the_post_in_flight_and_drops_the_queue() {
    let (entered_tx, entered) = channel();
    let s = setup(Behaviour::Block, false, Some(entered_tx));
    s.reporter.set_enabled(true);
    for i in 0..3 {
        s.reporter.capture(msg(&format!("event {i}")));
    }
    entered.recv_timeout(Duration::from_secs(5)).expect("the first post started");

    s.reporter.set_enabled(false);
    s.reporter.wait_idle();

    assert_eq!(s.counts.sent.load(Ordering::SeqCst), 0, "a post finished after reporting went off");
    assert_eq!(s.counts.attempts.load(Ordering::SeqCst), 1, "a queued event was tried after off");
    assert_eq!(spooled(&s), 0, "events queued before off are still spooled");
}

#[test]
fn control_switched_on_the_same_events_are_sent() {
    let s = setup(Behaviour::Accept, false, None);
    s.reporter.set_enabled(true);
    for i in 0..3 {
        s.reporter.capture(msg(&format!("event {i}")));
    }
    s.reporter.wait_idle();
    assert_eq!(s.counts.sent.load(Ordering::SeqCst), 3);
    assert_eq!(spooled(&s), 0);
}

#[test]
fn nothing_is_captured_or_spooled_while_off() {
    let s = setup(Behaviour::Accept, false, None);
    s.reporter.capture(msg("while off"));
    s.reporter.record_panic("boom while off");
    s.reporter.wait_idle();
    assert_eq!(spooled(&s), 0);
    assert_eq!(s.counts.attempts.load(Ordering::SeqCst), 0);
    // Control: the same panic while on is spooled (and then sent).
    s.reporter.set_enabled(true);
    s.reporter.wait_idle();
    s.reporter.record_panic("boom while on");
    assert_eq!(spooled(&s), 1);
}

fn crash_file(s: &Setup, name: &str, mtime: SystemTime) {
    let path = s.diag.join(name);
    std::fs::write(&path, "{\"app_name\":\"Cleat-rs\"}\n{\"exception\":{\"type\":\"EXC_BAD_ACCESS\",\"signal\":\"SIGSEGV\"}}").unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();
}

#[test]
fn crash_reports_from_before_the_switch_went_on_are_not_sent() {
    let s = setup(Behaviour::Accept, false, None);
    crash_file(&s, "Cleat-rs-old.ips", SystemTime::now() - Duration::from_secs(3600));
    // Control: one stamped after the switch goes on is picked up.
    crash_file(&s, "Cleat-rs-new.ips", SystemTime::now() + Duration::from_secs(3600));
    crash_file(&s, "Other-new.ips", SystemTime::now() + Duration::from_secs(3600));
    s.reporter.set_enabled(true);
    s.reporter.wait_idle();
    assert_eq!(s.counts.sent.load(Ordering::SeqCst), 1);
}

#[test]
fn the_probe_is_not_sent_while_off_and_is_sent_once_on() {
    let s = setup(Behaviour::Accept, true, None);
    s.reporter.set_enabled(false);
    s.reporter.wait_idle();
    assert_eq!(s.counts.attempts.load(Ordering::SeqCst), 0);
    assert_eq!(spooled(&s), 0);

    s.reporter.set_enabled(true);
    s.reporter.wait_idle();
    s.reporter.set_enabled(false);
    s.reporter.set_enabled(true);
    s.reporter.wait_idle();
    assert_eq!(s.counts.sent.load(Ordering::SeqCst), 1, "one probe per process");
}
