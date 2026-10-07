//! The one place decisions are made. Every listener and the config watcher deliver `Event`s to the
//! engine thread, and nothing touches engine state from anywhere else.

mod listeners;
mod run_loop;
mod status;

pub use run_loop::{run, Event, Origin, Timer};
pub use status::Status;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::SystemTime;

use run_loop::Scheduler;

use crate::audio::{AudioSystem, ListenerToken};
use crate::config::{watcher, Config};
use crate::model::{Action, DeviceSnapshot, Liveness};
use crate::rules::{balance, headphones_takeover, input_pin, input_volume, output_pin, reclaim};
use crate::state::clock::clock_ms;
use crate::state::EventLog;

const RECENT_EVENT_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Decide and log, never write CoreAudio.
    Observe,
    /// Write. Only constructed by tests in stage 1.
    Enforce,
}

pub struct Engine {
    pub(crate) system: Box<dyn AudioSystem>,
    mode: Mode,
    log: EventLog,
    config_path: PathBuf,
    status_path: PathBuf,
    pub(crate) config: Config,
    config_state: String,
    previous_device_uids: Option<HashSet<String>>,
    pub(crate) system_tokens: Vec<ListenerToken>,
    pub(crate) device_tokens: Vec<ListenerToken>,
    pub(crate) scheduler: Scheduler,
    recent_events: Vec<String>,
    listener_summary: Option<String>,
}

impl Engine {
    pub fn new(
        system: Box<dyn AudioSystem>,
        mode: Mode,
        log: EventLog,
        config_path: PathBuf,
        status_path: PathBuf,
    ) -> Self {
        Self {
            system,
            mode,
            log,
            config_path,
            status_path,
            config: Config::disabled(),
            config_state: "missing".into(),
            previous_device_uids: None,
            system_tokens: vec![],
            device_tokens: vec![],
            scheduler: Scheduler::default(),
            recent_events: vec![],
            listener_summary: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Loads the config, attaches listeners, starts the watcher (when given a sender) and runs the
    /// baseline pass.
    pub fn start(&mut self, watcher_tx: Option<Sender<Event>>) {
        self.load_config();
        let mode = match self.mode {
            Mode::Observe => "observe",
            Mode::Enforce => "enforce",
        };
        self.note(&format!(
            "engine started (config: {}, microphone: not requested (stage 1), mode: {mode})",
            self.config_state
        ));
        self.attach_system_listeners();
        self.rebind_devices();
        if let Some(tx) = watcher_tx {
            if let Err(e) = watcher::spawn(self.config_path.clone(), tx) {
                self.note(&format!("config: watcher not started ({e})"));
            }
        }
        self.reconcile(true, None);
    }

    /// A malformed or out-of-range file never replaces a good one; a missing file disables
    /// everything.
    fn load_config(&mut self) {
        if !self.config_path.exists() {
            self.config_state = "missing".into();
            self.config = Config::disabled();
            return;
        }
        match Config::load(&self.config_path) {
            Ok(c) => {
                self.config = c;
                self.config_state = "ok".into();
            }
            Err(e) => self.config_state = format!("invalid ({e})"),
        }
    }

    pub(crate) fn config_file_changed(&mut self, origin: Origin) {
        let previous = self.config.clone();
        let previous_state = self.config_state.clone();
        self.load_config();
        if self.config == previous && self.config_state == previous_state {
            return;
        }
        self.note(&format!("config: reloaded ({})", self.config_state));
        self.rebind_devices();
        self.reconcile(false, Some(origin));
    }

    /// One pass: snapshot, rules, apply, status. Always logs a `pass:` line with timings.
    pub fn reconcile(&mut self, consuming_arrivals: bool, origin: Option<Origin>) {
        let t0 = SystemTime::now();
        let mut snap = self.system.snapshot(&self.config);
        snap.arrived = self.arrivals(&snap, consuming_arrivals);
        snap.liveness = self.liveness_for_rules(&snap);
        let t1 = SystemTime::now();

        let config = &self.config;
        let mut actions = input_pin::reconcile(&snap, config);
        let output = if headphones_takeover::has_eligible_arrival(&snap, config) {
            headphones_takeover::reconcile(&snap, config)
        } else {
            output_pin::reconcile(&snap, config)
        };
        let balance = if output.is_empty() { balance::reconcile(&snap, config) } else { vec![] };
        actions.extend(output);
        actions.extend(balance);
        actions.extend(input_volume::reconcile(&snap, config));
        // Stage 1 has no pairing list, so reclaim never has a headset to ask for.
        actions.extend(reclaim::reconcile(&snap, &[], config, &HashSet::new()));
        let t2 = SystemTime::now();

        let us = |a: SystemTime, b: SystemTime| b.duration_since(a).map_or(0, |d| d.as_micros());
        let (label, recv, latency, delay) = match origin {
            Some(o) => (
                o.label,
                clock_ms(o.received),
                t2.duration_since(o.received).map_or("-".into(), |d| format!("{:.3}", d.as_secs_f64() * 1000.0)),
                o.delay_ms.to_string(),
            ),
            None => ("startup", "-".into(), "-".into(), "-".into()),
        };
        self.note(&format!(
            "pass: trigger={label} recv={recv} decided={} latency_ms={latency} delay_ms={delay} snapshot_us={} rules_us={} actions={}{}",
            clock_ms(t2),
            us(t0, t1),
            us(t1, t2),
            actions.len(),
            if actions.is_empty() { " (nothing to do)" } else { "" }
        ));
        for a in &actions {
            self.apply(a);
        }
        self.write_status(&snap);
    }

    /// Which devices are new since the last consuming pass. Empty on the first pass.
    fn arrivals(&mut self, snap: &DeviceSnapshot, consuming: bool) -> HashSet<String> {
        let present: HashSet<String> = snap.devices.iter().map(|d| d.uid.clone()).collect();
        let arrived = match &self.previous_device_uids {
            Some(prev) => present.difference(prev).cloned().collect(),
            None => HashSet::new(),
        };
        if consuming {
            self.previous_device_uids = Some(present);
        }
        arrived
    }

    /// Stage 1 measures nothing (the same as Swift with the microphone denied): every device is
    /// untracked, which the input rule reads as present.
    fn liveness_for_rules(&self, _snap: &DeviceSnapshot) -> HashMap<String, Liveness> {
        HashMap::new()
    }

    /// The only caller of the `AudioSystem` writers. In observe mode it returns before any of them.
    fn apply(&mut self, action: &Action) {
        if let Action::RequestRoute { reason, .. } = action {
            self.note(&format!("reclaim: {reason} [stage 1: request not sent]"));
            return;
        }
        if self.mode == Mode::Observe {
            self.note(&format!("{}: {} [observe: not applied]", action.label(), action.reason()));
            return;
        }
        let status = match *action {
            Action::SetDefaultInput(id, _) => self.system.set_default_input(id),
            Action::SetDefaultOutput(id, _) => self.system.set_default_output(id),
            Action::SetBalance(id, v, _) => self.system.set_balance(id, v),
            Action::SetInputVolume(id, v, _) => self.system.set_input_volume(id, v),
            Action::RequestRoute { .. } => return,
        };
        if status == 0 {
            self.note(&format!("{}: {}", action.label(), action.reason()));
        } else {
            self.note(&format!("{}: {} failed (OSStatus {status})", action.label(), action.reason()));
        }
    }

    /// Appends to the event log and to the ring the status file carries.
    pub fn note(&mut self, msg: &str) {
        let line = self.log.append(msg);
        self.recent_events.push(line);
        if self.recent_events.len() > RECENT_EVENT_LIMIT {
            let excess = self.recent_events.len() - RECENT_EVENT_LIMIT;
            self.recent_events.drain(..excess);
        }
    }
}
