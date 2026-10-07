//! "Volume update from PID" lines from coreaudiod's Bluetooth audio driver (Swift
//! `VolumeWriterLog.swift`, 0.3.10). Driver debug text, not an API: when it stops matching, the
//! hold stops undoing anything rather than undoing everything.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::engine::Event;
use crate::rules::output_volume_hold::TOLERANCE;

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeWrite {
    /// The log's own timestamp, so a line that arrives late is still placed where it happened.
    pub at: SystemTime,
    pub pid: i32,
    /// Executable path as resolved on arrival ("pid N" when it had already exited); the engine
    /// replaces it with the config entry it matched, or its last component.
    pub writer: String,
    pub control: i64,
    pub from: f32,
    pub to: f32,
}

impl VolumeWrite {
    pub fn changes_value(&self) -> bool {
        (self.from - self.to).abs() > TOLERANCE
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeWriterEvent {
    Write(VolumeWrite),
    /// Passed the predicate but did not parse: the driver's wording changed.
    Unrecognised(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    timestamp: Option<String>,
    event_message: Option<String>,
}

/// None for lines that are not log events at all (the plain-text "Filtering ..." header).
pub fn parse_ndjson(line: &str, writer: impl Fn(i32) -> String) -> Option<VolumeWriterEvent> {
    let decoded: Line = serde_json::from_str(line).ok()?;
    let message = decoded.event_message?;
    let (Some(at), Some((pid, control, from, to))) =
        (decoded.timestamp.as_deref().and_then(parse_timestamp), parse_message(&message))
    else {
        return Some(VolumeWriterEvent::Unrecognised(message));
    };
    Some(VolumeWriterEvent::Write(VolumeWrite { at, pid, writer: writer(pid), control, from, to }))
}

/// `PID = (\d+) Control ID = (\d+) Scalar volume ([0-9.]+) -> ([0-9.]+)`, without a regex crate.
pub fn parse_message(message: &str) -> Option<(i32, i64, f32, f32)> {
    let rest = &message[message.find("PID = ")? + 6..];
    let (pid, rest) = take(rest, |c| c.is_ascii_digit())?;
    let rest = rest.strip_prefix(" Control ID = ")?;
    let (control, rest) = take(rest, |c| c.is_ascii_digit())?;
    let rest = rest.strip_prefix(" Scalar volume ")?;
    let (from, rest) = take(rest, |c| c.is_ascii_digit() || c == '.')?;
    let rest = rest.strip_prefix(" -> ")?;
    let (to, _) = take(rest, |c| c.is_ascii_digit() || c == '.')?;
    Some((pid.parse().ok()?, control.parse().ok()?, from.parse().ok()?, to.parse().ok()?))
}

fn take(s: &str, f: impl Fn(char) -> bool) -> Option<(&str, &str)> {
    let end = s.find(|c: char| !f(c)).unwrap_or(s.len());
    (end > 0).then(|| s.split_at(end))
}

/// `yyyy-MM-dd HH:mm:ss.SSSSSS+zzzz`.
pub fn parse_timestamp(s: &str) -> Option<SystemTime> {
    let b = s.as_bytes();
    if b.len() != 31 || b[10] != b' ' || b[19] != b'.' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    // SAFETY: zeroed tm is valid input once the fields below are set.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = (n(0..4)? - 1900) as i32;
    tm.tm_mon = (n(5..7)? - 1) as i32;
    tm.tm_mday = n(8..10)? as i32;
    tm.tm_hour = n(11..13)? as i32;
    tm.tm_min = n(14..16)? as i32;
    tm.tm_sec = n(17..19)? as i32;
    let micros = n(20..26)?;
    let sign = match b[26] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let offset = sign * (n(27..29)? * 3600 + n(29..31)? * 60);
    // SAFETY: tm is a valid, initialised struct.
    let secs = unsafe { libc::timegm(&mut tm) } - offset;
    (secs >= 0).then(|| UNIX_EPOCH + Duration::from_secs(secs as u64) + Duration::from_micros(micros as u64))
}

/// `proc_pidpath` of a process; "pid N" when it has already exited.
pub fn executable_path(pid: i32) -> String {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: buffer of the documented maximum size.
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return format!("pid {pid}");
    }
    String::from_utf8_lossy(&buf[..n as usize]).into_owned()
}

/// Where writer lines come from. A trait so the engine can be driven by a script in tests.
pub trait VolumeWriterSource {
    /// Starts delivering `Event::VolumeWriter` / `VolumeWriterExited`. False when it cannot start.
    fn start(&mut self) -> bool;
    fn stop(&mut self);
}

pub const PREDICATE: &str = r#"process == "coreaudiod" AND eventMessage CONTAINS "Volume update from PID""#;

/// `/usr/bin/log stream` as a child process in its own group.
pub struct LogStream {
    tx: Sender<Event>,
    pid: Option<i32>,
    stopped: Arc<AtomicBool>,
}

impl LogStream {
    pub fn new(tx: Sender<Event>) -> Self {
        Self { tx, pid: None, stopped: Arc::new(AtomicBool::new(false)) }
    }

    /// A Cleat killed without `stop()` leaves its `log stream` reparented to launchd: only those go.
    fn reap_orphans() {
        // SAFETY: getuid cannot fail.
        let uid = unsafe { libc::getuid() }.to_string();
        let _ = Command::new("/usr/bin/pkill")
            .args(["-P", "1", "-U", &uid, "-f", "^/usr/bin/log stream .*Volume update from PID"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl VolumeWriterSource for LogStream {
    fn start(&mut self) -> bool {
        Self::reap_orphans();
        let Ok(mut child) = Command::new("/usr/bin/log")
            .args(["stream", "--style", "ndjson", "--predicate", PREDICATE])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
        else {
            return false;
        };
        let Some(stdout) = child.stdout.take() else { return false };
        self.pid = Some(child.id() as i32);
        let stopped = Arc::new(AtomicBool::new(false));
        self.stopped = stopped.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // Named on arrival: a short-lived writer may be gone by the time the engine looks.
                if let Some(ev) = parse_ndjson(&line, executable_path) {
                    let _ = tx.send(Event::VolumeWriter(ev));
                }
            }
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            if !stopped.load(Ordering::SeqCst) {
                let _ = tx.send(Event::VolumeWriterExited(code));
            }
        });
        true
    }

    fn stop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(pid) = self.pid.take() {
            // SAFETY: the child leads its own group (process_group(0)); signalling it ends the read.
            unsafe { libc::kill(-pid, libc::SIGTERM) };
        }
    }
}

impl Drop for LogStream {
    fn drop(&mut self) {
        self.stop();
    }
}
