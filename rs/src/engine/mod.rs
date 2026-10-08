//! The one place decisions are made. Every listener and the config watcher deliver `Event`s to the
//! engine thread, and nothing touches engine state from anywhere else (Swift `Engine.swift`).

mod eviction;
mod launch;
mod listeners;
mod liveness;
mod output_volume;
mod reclaim;
mod run_loop;
mod status;

pub use eviction::{EVICTION_LIMIT, EVICTION_WINDOW};
pub use listeners::{consumes_arrivals, BALANCE_SETTLE_MS, RETRY_MS, SETTLE_MS};
pub use reclaim::{
    RECLAIM_BACKOFF, RECLAIM_INTERVAL, RECLAIM_RETRY_DELAY, RECLAIM_RETRY_SPAN, RECLAIM_SCORE, RETURN_TIMEOUT,
};
pub use output_volume::{OutputVolumeStatus, WriterSourceFactory, OUTPUT_VOLUME_SETTLE_MS};
pub use run_loop::{run, Event, Origin, Timer};
pub use status::Status;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

use run_loop::Scheduler;

use crate::audio::{AudioSystem, ListenerToken};
use crate::config::{watcher, Config};
use crate::identity::Identity;
use crate::launch::{AgentService, Launchd};
use crate::liveness::{DetectorFactory, LivenessDetecting};
use crate::model::{Action, DeviceSnapshot, Liveness, MicrophonePermission};
use crate::reclaim::{BluetoothInventory, RouteRequesting};
use crate::rules::{balance, headphones_takeover, input_pin, input_volume, output_pin};
use crate::state::clock::{clock_ms, Clock};
use crate::state::reaction_clock::ReactionClock;
use crate::state::EventLog;

const RECENT_EVENT_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Decide and log, never write: no CoreAudio writes, no launch agent, no error reports.
    Observe,
    /// The daemon.
    Enforce,
}

/// Everything outside the engine, so each face can be replaced in tests.
pub struct EngineDeps {
    pub system: Box<dyn AudioSystem>,
    pub log: EventLog,
    pub config_path: PathBuf,
    pub status_path: PathBuf,
    pub clock: Box<dyn Clock>,
    pub identity: Identity,
    /// The bundle's launchd agent.
    pub agent: Box<dyn AgentService>,
    /// The app as a login item, which versions up to 0.3.2 registered and this retires.
    pub login_item: Box<dyn AgentService>,
    pub launchd: Box<dyn Launchd>,
    /// Told when `errorReports` changes.
    pub error_reports_changed: Box<dyn FnMut(bool)>,
    /// Makes a silence detector for a device; flips come back as `Event::LivenessFlip`.
    pub detectors: DetectorFactory,
    /// The routing SPI; answers come back as `Event::RouteAnswered`.
    pub routing: Box<dyn RouteRequesting>,
    /// The paired Bluetooth devices.
    pub inventory: Box<dyn BluetoothInventory>,
    /// Makes the output volume hold's writer source (`log stream`).
    pub writer_source: WriterSourceFactory,
    /// Where the config watcher sends; None runs without a watcher (tests).
    pub events: Option<Sender<Event>>,
}

pub struct Engine {
    pub(crate) system: Box<dyn AudioSystem>,
    mode: Mode,
    /// Writes the `pass:`, `event:` and `listeners:` diagnostics. Always on in observe mode.
    trace: bool,
    log: EventLog,
    config_path: PathBuf,
    status_path: PathBuf,
    clock: Box<dyn Clock>,
    identity: Identity,
    agent: Box<dyn AgentService>,
    login_item: Box<dyn AgentService>,
    launchd: Box<dyn Launchd>,
    error_reports_changed: Box<dyn FnMut(bool)>,
    detectors: DetectorFactory,
    routing: Box<dyn RouteRequesting>,
    inventory: Box<dyn BluetoothInventory>,
    reclaim: reclaim::ReclaimBook,
    eviction: eviction::EvictionBook,
    writer_source: WriterSourceFactory,
    outvol: output_volume::OutputVolumeHold,
    events: Option<Sender<Event>>,
    pub(crate) config: Config,
    config_state: String,
    microphone: MicrophonePermission,
    /// Silence verdict per device UID, owned here because the detectors are.
    liveness_state: HashMap<String, Liveness>,
    liveness_detectors: HashMap<String, Box<dyn LivenessDetecting>>,
    /// UIDs whose detector could not be started; membership keeps the retry quiet.
    liveness_unavailable: HashSet<String>,
    error_reports_applied: bool,
    previous_device_uids: Option<HashSet<String>>,
    pub(crate) system_tokens: Vec<ListenerToken>,
    pub(crate) device_tokens: Vec<ListenerToken>,
    pub(crate) scheduler: Scheduler,
    /// When the balance listener last fired, on the scheduler's clock.
    pub(crate) balance_changed_at: Option<Duration>,
    recent_events: Vec<String>,
    listener_summary: Option<String>,
    /// Event-to-write-back timing, published as status.json `performance`.
    pub(crate) reactions: ReactionClock,
}

impl Engine {
    pub fn new(deps: EngineDeps, mode: Mode, trace: bool) -> Self {
        Self {
            system: deps.system,
            mode,
            trace: trace || mode == Mode::Observe,
            log: deps.log,
            config_path: deps.config_path,
            status_path: deps.status_path,
            clock: deps.clock,
            identity: deps.identity,
            agent: deps.agent,
            login_item: deps.login_item,
            launchd: deps.launchd,
            error_reports_changed: deps.error_reports_changed,
            detectors: deps.detectors,
            routing: deps.routing,
            inventory: deps.inventory,
            reclaim: reclaim::ReclaimBook::default(),
            eviction: eviction::EvictionBook::default(),
            writer_source: deps.writer_source,
            outvol: output_volume::OutputVolumeHold::default(),
            events: deps.events,
            config: Config::disabled(),
            config_state: "missing".into(),
            microphone: MicrophonePermission::Pending,
            liveness_state: HashMap::new(),
            liveness_detectors: HashMap::new(),
            liveness_unavailable: HashSet::new(),
            error_reports_applied: false,
            previous_device_uids: None,
            system_tokens: vec![],
            device_tokens: vec![],
            scheduler: Scheduler::default(),
            balance_changed_at: None,
            recent_events: vec![],
            listener_summary: None,
            reactions: ReactionClock::default(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Order follows Swift `Engine.start`: config, the started line, launch agent and error
    /// reports (enforce only), listeners, watcher, then the baseline pass.
    pub fn start(&mut self, microphone: MicrophonePermission) {
        self.microphone = microphone;
        self.load_config();
        let observe = if self.mode == Mode::Observe { ", mode: observe" } else { "" };
        self.note(&format!(
            "engine started (config: {}, microphone: {}{observe})",
            self.config_state,
            self.microphone.label()
        ));
        self.sync_side_effects();
        self.attach_system_listeners();
        self.rebind_devices();
        if let Some(tx) = self.events.clone() {
            if let Err(e) = watcher::spawn(self.config_path.clone(), tx) {
                self.note(&format!("config: watcher not started ({e})"));
            }
        }
        self.reconcile(true, None);
    }

    /// Launch agent and error reports. Observe mode touches neither (D6).
    fn sync_side_effects(&mut self) {
        if self.mode == Mode::Observe {
            return;
        }
        self.sync_launch_at_login();
        self.hand_over_to_launch_agent_if_needed();
        self.sync_error_reports();
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
        self.eviction.config_changed();
        self.sync_side_effects();
        self.rebind_devices();
        self.reconcile(false, Some(origin));
    }

    /// Passes `errorReports` on when it changes.
    fn sync_error_reports(&mut self) {
        let wanted = self.config.error_reports;
        if wanted == self.error_reports_applied {
            return;
        }
        self.error_reports_applied = wanted;
        (self.error_reports_changed)(wanted);
        self.note(&format!("errorReports: {}", if wanted { "on" } else { "off" }));
    }

    /// One pass: snapshot, rules, apply, status. The `pass:` line is written in trace mode only.
    pub fn reconcile(&mut self, consuming_arrivals: bool, origin: Option<Origin>) {
        let t0 = SystemTime::now();
        let mut snap = self.system.snapshot(&self.config);
        snap.arrived = self.arrivals(&snap, consuming_arrivals);
        // Also the retry for a detector whose start failed on a device not yet ready.
        self.sync_liveness_detectors(&snap);
        snap.liveness = self.liveness_for_rules(&snap);
        self.eviction.observe(&mut snap);
        let now = self.clock.mono();
        let t1 = SystemTime::now();

        let input = input_pin::reconcile(&snap, &self.config);
        let (mut actions, mut held) = self.eviction.gate(eviction::Side::Input, input, &snap, &self.config, now);
        let output = if headphones_takeover::has_eligible_arrival(&snap, &self.config) {
            headphones_takeover::reconcile(&snap, &self.config)
        } else {
            output_pin::reconcile(&snap, &self.config)
        };
        let (output, held_out) = self.eviction.gate(eviction::Side::Output, output, &snap, &self.config, now);
        held.extend(held_out);
        // A revert waits while the output itself is being moved, and goes before the balance.
        let hold = if output.is_empty() { self.output_volume_actions(&snap) } else { vec![] };
        let config = &self.config;
        // Not while a balance change is settling: an app may have written one channel and not
        // yet the other. Any pass can land in that window; the settle beat judges it.
        let settling = self
            .balance_changed_at
            .is_some_and(|at| self.clock.mono() < at + Duration::from_millis(BALANCE_SETTLE_MS));
        let balance = if output.is_empty() && hold.is_empty() && !settling {
            balance::reconcile(&snap, config)
        } else {
            vec![]
        };
        actions.extend(output);
        actions.extend(hold);
        actions.extend(balance);
        actions.extend(input_volume::reconcile(&snap, config));
        actions.extend(self.reclaim_requests(&snap));
        let t2 = SystemTime::now();

        if self.trace {
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
        }
        for line in &held {
            self.note(line);
        }
        for a in &actions {
            self.apply(a);
        }
        self.check_reclaim_returns(&snap);
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

    /// The only caller of the `AudioSystem` writers. Observe mode returns before any of them.
    /// A failure is logged with its OSStatus and not retried.
    fn apply(&mut self, action: &Action) {
        if self.mode == Mode::Observe {
            self.note(&format!("{}: {} [observe: not applied]", action.label(), action.reason()));
            return;
        }
        let status = match action {
            Action::SetDefaultInput(id, _) => self.system.set_default_input(*id),
            Action::SetDefaultOutput(id, _) => {
                self.note_own_output_write(*id);
                self.system.set_default_output(*id)
            }
            Action::SetBalance(id, v, _) => self.system.set_balance(*id, *v),
            Action::SetInputVolume(id, v, _) => self.system.set_input_volume(*id, *v),
            Action::SetOutputVolume(id, v, _) => self.system.set_output_volume(*id, *v),
            // A question for another daemon: its line is written when the answer arrives.
            Action::RequestRoute { name, address, reason } => {
                self.request_route(name, address, reason);
                return;
            }
        };
        if status == 0 {
            self.note(&format!("{}: {}", action.label(), action.reason()));
            let (mono, wall) = (self.uptime(), self.clock.wall());
            self.reactions.wrote(mono, wall);
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
