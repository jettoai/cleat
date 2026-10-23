//! HEIC to JPEG for the files that arrive in a watched folder (AirDrop into Downloads). Its own
//! thread does all of it: the engine only starts and stops it and writes what it reports.
//!
//! A folder change wakes the worker, which waits for the changes to stop, lists the first level,
//! and converts each new HEIC once its size has held still for a moment. Nothing runs while the
//! folder is quiet.

pub mod convert;
pub mod plan;
mod watcher;

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::{paths, HeicConfig};
use crate::engine::Event;
use plan::{Entry, Skip};
use watcher::Watcher;

/// How long the worker waits, injectable so tests do not wait for real.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Quiet time after the last folder change before the folder is listed.
    pub debounce: Duration,
    /// A file must keep its size this long before it is converted.
    pub stable: Duration,
    /// Waits before the three retries of a picture that will not decode.
    pub retries: [Duration; 3],
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(1500),
            stable: Duration::from_secs(1),
            retries: [Duration::from_secs(2), Duration::from_secs(5), Duration::from_secs(15)],
        }
    }
}

/// What the worker tells the engine, one log line each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    Converted { from: String, to: String, width: usize, height: usize, ms: u128, originals: String },
    Skipped { name: String, reason: &'static str },
    Failed { name: String, reason: String },
    NoAccess { path: String },
    /// The folder can be read again after `NoAccess`.
    Readable { path: String },
}

impl Report {
    pub fn line(&self) -> String {
        match self {
            Report::Converted { from, to, width, height, ms, originals } => {
                format!("heic: {from} -> {to} {width}x{height} {ms} ms, original -> {originals}/")
            }
            Report::Skipped { name, reason } => format!("heic: skipped {name} ({reason})"),
            Report::Failed { name, reason } => format!("heic: failed {name}: {reason}"),
            Report::NoAccess { path } => format!("heic: no access to {path}"),
            Report::Readable { path } => format!("heic: {path} can be read again"),
        }
    }
}

/// A running watch. Dropping it stops the folder watch at once and the worker after the file it is on.
pub struct Handle {
    // Field order is drop order: the watcher goes first, then the sender whose closing ends the worker.
    _watcher: Watcher,
    _wake: Sender<()>,
}

/// Starts watching; what happens afterwards comes back as `Event::Photos`. Only HEICs born after
/// this call are converted.
pub fn start(config: &HeicConfig, home: &Path, events: Sender<Event>, timing: Timing) -> Option<Handle> {
    let watch = config.watch_dir(home);
    let originals = config.originals_dir(home);
    if same_folder(&watch, &originals) {
        let reason = "the originals folder is the watched folder".to_string();
        let _ = events.send(Event::Photos(Report::Failed { name: paths::tilde(&watch), reason }));
        return None;
    }
    // Read before anything runs: what is here now is not new, whatever is added after.
    let baseline = names(&watch).into_iter().filter(|n| plan::is_heic_name(n)).collect();
    let (wake, rx) = mpsc::channel();
    let watcher = match Watcher::spawn(watch.clone(), wake.clone()) {
        Ok(w) => w,
        Err(e) => {
            let _ = events.send(Event::Photos(Report::Failed { name: paths::tilde(&watch), reason: e.to_string() }));
            return None;
        }
    };
    let worker = Worker {
        originals,
        originals_label: config.originals.clone(),
        watch,
        long_edge: config.long_edge,
        quality: config.quality,
        started: SystemTime::now(),
        timing,
        rx,
        events,
        baseline,
        handled: HashMap::new(),
        noted: HashSet::new(),
        folder_error: None,
        woken: false,
    };
    let _ = wake.send(()); // look once at the start: it is how a folder that cannot be read gets reported
    std::thread::Builder::new().name("cleat-photos".into()).spawn(move || worker.run()).ok()?;
    Some(Handle { _watcher: watcher, _wake: wake })
}

/// Whether two paths are the same folder: through links when both exist, else by their spelling
/// (`dir/`, `dir/.` and `dir` agree).
pub fn same_folder(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.components().eq(b.components()),
    }
}

/// The names in a folder's first level; none when it cannot be read.
fn names(dir: &Path) -> HashSet<String> {
    let Ok(listing) = std::fs::read_dir(dir) else { return HashSet::new() };
    listing.flatten().filter_map(|i| i.file_name().into_string().ok()).collect()
}

/// Longest a file is waited on for its size to settle.
const MAX_SETTLE_CHECKS: u32 = 120;

struct Worker {
    watch: PathBuf,
    originals: PathBuf,
    /// The originals folder as the config names it, for the log line.
    originals_label: String,
    long_edge: u32,
    quality: u32,
    started: SystemTime,
    /// The HEIC names that were in the folder when the watch started.
    baseline: HashSet<String>,
    timing: Timing,
    rx: Receiver<()>,
    events: Sender<Event>,
    /// Files that failed, by the size they had: tried again only if it changes. A converted file is
    /// not kept here, its name may come back with a new picture.
    handled: HashMap<String, u64>,
    /// Skips already reported.
    noted: HashSet<String>,
    /// Why the folder could not be read at the last look; the same reason is reported once.
    folder_error: Option<ErrorKind>,
    /// A folder change arrived while the worker was busy.
    woken: bool,
}

impl Worker {
    fn report(&self, report: Report) {
        let _ = self.events.send(Event::Photos(report));
    }

    fn run(mut self) {
        while self.wait_for_quiet() && self.scan() {}
    }

    /// Blocks for a change, then until changes stop for `debounce`. False once the watch is gone.
    fn wait_for_quiet(&mut self) -> bool {
        if !std::mem::take(&mut self.woken) && self.rx.recv().is_err() {
            return false;
        }
        loop {
            match self.rx.recv_timeout(self.timing.debounce) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => return true,
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        }
    }

    /// Waits `d`, noting changes. False once the watch is gone.
    fn pause(&mut self, d: Duration) -> bool {
        let end = Instant::now() + d;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            match self.rx.recv_timeout(left) {
                Ok(()) => self.woken = true,
                Err(RecvTimeoutError::Timeout) => return true,
                Err(RecvTimeoutError::Disconnected) => return false,
            }
        }
    }

    /// One look at the folder and every job it yields. False once the watch is gone.
    fn scan(&mut self) -> bool {
        let listing = match std::fs::read_dir(&self.watch) {
            Ok(l) => l,
            Err(e) => {
                if self.folder_error != Some(e.kind()) {
                    self.folder_error = Some(e.kind());
                    let path = paths::tilde(&self.watch);
                    self.report(match e.kind() {
                        ErrorKind::PermissionDenied => Report::NoAccess { path },
                        _ => Report::Failed { name: path, reason: e.to_string() },
                    });
                }
                return true;
            }
        };
        if self.folder_error.take() == Some(ErrorKind::PermissionDenied) {
            self.report(Report::Readable { path: paths::tilde(&self.watch) });
        }
        let mut existing = HashSet::new();
        let mut entries = vec![];
        for item in listing.flatten() {
            let Ok(name) = item.file_name().into_string() else { continue };
            if plan::is_heic_name(&name) {
                let Ok(meta) = item.metadata() else { continue };
                if self.handled.get(&name) != Some(&meta.len()) {
                    let born = meta.created().unwrap_or(UNIX_EPOCH);
                    entries.push(Entry { name: name.clone(), size: meta.len(), born, is_dir: !meta.is_file(),
                        in_baseline: self.baseline.contains(&name),
                    });
                }
            }
            existing.insert(name);
        }
        let plan = plan::plan(&entries, self.started, &existing, &names(&self.originals));
        for (name, skip) in plan.skipped {
            if skip != Skip::Older && self.noted.insert(name.clone()) {
                self.report(Report::Skipped { name, reason: skip.reason() });
            }
        }
        for job in plan.jobs {
            self.handled.insert(job.source.clone(), job.size);
            if !self.process(&job) {
                return false;
            }
        }
        true
    }

    /// Waits for the size to settle, converts with retries, files the original. False once the
    /// watch is gone.
    fn process(&mut self, job: &plan::Job) -> bool {
        let source = self.watch.join(&job.source);
        let mut size = job.size;
        for _ in 0..MAX_SETTLE_CHECKS {
            if !self.pause(self.timing.stable) {
                return false;
            }
            let Ok(meta) = std::fs::metadata(&source) else { return true }; // taken away meanwhile
            if meta.len() == size {
                break;
            }
            size = meta.len();
            self.handled.insert(job.source.clone(), size);
        }
        let temp = self.watch.join(format!(".{}.cleat-tmp", job.output));
        let mut reason = String::new();
        for attempt in 0..=self.timing.retries.len() {
            let began = Instant::now();
            match convert::to_jpeg(&source, &temp, self.long_edge, self.quality) {
                Ok((width, height)) => {
                    let ms = began.elapsed().as_millis();
                    let jpeg = self.watch.join(&job.output);
                    match convert::finish(&temp, &jpeg, &source, &self.originals.join(&job.original)) {
                        Ok((to, _)) => {
                            self.handled.remove(&job.source);
                            self.report(Report::Converted {
                                from: job.source.clone(),
                                to,
                                width,
                                height,
                                ms,
                                originals: self.originals_label.clone(),
                            });
                        }
                        Err(e) => self.report(Report::Failed {
                            name: job.source.clone(),
                            reason: format!("converted to {} but could not file the original: {e}", job.output),
                        }),
                    }
                    return true;
                }
                Err(e) => reason = e,
            }
            if let Some(&wait) = self.timing.retries.get(attempt) {
                if !self.pause(wait) {
                    return false;
                }
            }
        }
        self.report(Report::Failed {
            name: job.source.clone(),
            reason: format!("{reason}; left in place after {} retries", self.timing.retries.len()),
        });
        true
    }
}
