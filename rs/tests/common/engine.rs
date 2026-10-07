//! Engine doubles (Swift `EngineTestDoubles.swift`) and a harness that drives a real `Engine` on a
//! fake clock: `drive(ms)` is Swift's `waitOnQueue`, without the wait.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cleat_rs::audio::{AudioSystem, ListenTarget, ListenerKind, ListenerToken};
use cleat_rs::config::Config;
use cleat_rs::engine::{Engine, EngineDeps, Event, Mode, Status};
use cleat_rs::identity::Identity;
use cleat_rs::launch::{AgentService, AgentStatus, Launchd};
use cleat_rs::liveness::LivenessDetecting;
use cleat_rs::model::{BluetoothHeadset, DeviceSnapshot, MicrophonePermission};
use cleat_rs::reclaim::{BluetoothInventory, RouteRequesting, RouteResponse};
use cleat_rs::state::clock::Clock;
use cleat_rs::state::EventLog;

/// The audio system as a value. Writes land back in the snapshot, so a second pass converges.
pub struct FakeAudioState {
    pub snapshot: RefCell<DeviceSnapshot>,
    pub writes: RefCell<Vec<String>>,
    /// False is a device that has just appeared: `setDefaultOutput` succeeds and does not stick.
    pub output_writes_stick: Cell<bool>,
    /// `owns_bluetooth_audio` per device id; absent is an unreadable property.
    pub owns: RefCell<std::collections::HashMap<u32, bool>>,
}

pub struct FakeAudio(pub Rc<FakeAudioState>);

impl AudioSystem for FakeAudio {
    fn snapshot(&self, _config: &Config) -> DeviceSnapshot {
        self.0.snapshot.borrow().clone()
    }
    fn set_default_input(&self, id: u32) -> i32 {
        self.0.writes.borrow_mut().push(format!("input:{id}"));
        self.0.snapshot.borrow_mut().default_input = Some(id);
        0
    }
    fn set_default_output(&self, id: u32) -> i32 {
        self.0.writes.borrow_mut().push(format!("output:{id}"));
        if self.0.output_writes_stick.get() {
            self.0.snapshot.borrow_mut().default_output = Some(id);
        }
        0
    }
    fn set_balance(&self, id: u32, value: f32) -> i32 {
        self.0.writes.borrow_mut().push(format!("balance:{id}:{value:.2}"));
        self.0.snapshot.borrow_mut().output_balance = Some(value);
        0
    }
    fn set_input_volume(&self, id: u32, value: f32) -> i32 {
        self.0.writes.borrow_mut().push(format!("volume:{id}:{value:.2}"));
        self.0.snapshot.borrow_mut().input_volumes.insert(id, value);
        0
    }
    fn set_output_volume(&self, id: u32, value: f32) -> i32 {
        self.0.writes.borrow_mut().push(format!("outvol:{id}:{value:.4}"));
        let mut s = self.0.snapshot.borrow_mut();
        let channels = s.output_volumes.len().max(2);
        s.output_volumes = vec![value; channels];
        0
    }
    fn nominal_sample_rate(&self, _id: u32) -> Option<f64> {
        None
    }
    fn add_listener(&self, _target: ListenTarget) -> Option<ListenerToken> {
        None
    }
    fn remove_listener(&self, _token: ListenerToken) {}
    fn owns_bluetooth_audio(&self, device: u32) -> Option<bool> {
        self.0.owns.borrow().get(&device).copied()
    }
}

#[derive(Clone, Default)]
pub struct FakeClock(pub Rc<Cell<Duration>>);

impl Clock for FakeClock {
    fn mono(&self) -> Duration {
        self.0.get()
    }
    /// Deterministic: a fixed origin plus the monotonic offset, so log-time windows can be driven.
    fn wall(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_791_339_595) + self.0.get()
    }
}

pub struct FakeAgentState {
    pub status: Cell<AgentStatus>,
    pub registers: Cell<usize>,
    pub unregisters: Cell<usize>,
    pub status_reads: Cell<usize>,
}

impl FakeAgentState {
    pub fn new(status: AgentStatus) -> Rc<Self> {
        Rc::new(Self { status: Cell::new(status), registers: Cell::new(0), unregisters: Cell::new(0), status_reads: Cell::new(0) })
    }
}

pub struct FakeAgent(pub Rc<FakeAgentState>);

impl AgentService for FakeAgent {
    fn status(&self) -> AgentStatus {
        self.0.status_reads.set(self.0.status_reads.get() + 1);
        self.0.status.get()
    }
    fn register(&self) -> Result<(), String> {
        self.0.registers.set(self.0.registers.get() + 1);
        self.0.status.set(AgentStatus::Enabled);
        Ok(())
    }
    fn unregister(&self) -> Result<(), String> {
        self.0.unregisters.set(self.0.unregisters.get() + 1);
        self.0.status.set(AgentStatus::NotRegistered);
        Ok(())
    }
}

/// No job loaded, nothing to hand over to.
pub struct FakeLaunchd;

impl Launchd for FakeLaunchd {
    fn loaded_job(&self) -> (bool, Option<i32>) {
        (false, None)
    }
    fn kickstart(&self) -> (i32, String) {
        (1, String::new())
    }
    fn started_by_launchd(&self) -> bool {
        false
    }
    fn exit(&self) {
        panic!("the engine tried to exit");
    }
}

/// The HAL side of silence detection as a script (Swift `DetectorLog`). Answers for successive
/// `start()` calls; the last one repeats once the script runs out, `true` when it is empty.
#[derive(Default)]
pub struct DetectorLog {
    pub start_results: Vec<bool>,
    pub made: Cell<usize>,
    pub start_count: Cell<usize>,
    pub stop_count: Cell<usize>,
}

impl DetectorLog {
    fn next_start_result(&self) -> bool {
        let n = self.start_count.get();
        self.start_count.set(n + 1);
        self.start_results.get(n).or(self.start_results.last()).copied().unwrap_or(true)
    }
}

pub struct FakeDetector {
    device_id: u32,
    name: String,
    zero_seconds: f64,
    log: Rc<DetectorLog>,
}

impl LivenessDetecting for FakeDetector {
    fn device_id(&self) -> u32 {
        self.device_id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn zero_seconds(&self) -> f64 {
        self.zero_seconds
    }
    fn start(&mut self) -> bool {
        self.log.next_start_result()
    }
    fn stop(&mut self) {
        self.log.stop_count.set(self.log.stop_count.get() + 1);
    }
}

/// The routing daemon as one canned answer (Swift `FakeRouting`). `None` is a request that is
/// never answered. Answers queue in `delivered` until the harness hands them to the engine, which
/// is what the real client's GCD hop does.
pub struct FakeRoutingState {
    pub available: bool,
    pub answer: RefCell<Option<RouteResponse>>,
    pub addresses: RefCell<Vec<String>>,
    pub scores: RefCell<Vec<i32>>,
    pub reasons: RefCell<Vec<String>>,
    pub delivered: RefCell<Vec<(String, String, RouteResponse)>>,
}

pub struct FakeRouting(pub Rc<FakeRoutingState>);

impl RouteRequesting for FakeRouting {
    fn is_available(&self) -> bool {
        self.0.available
    }
    fn request(&mut self, name: &str, address: &str, score: i32, reason: &str) {
        self.0.addresses.borrow_mut().push(address.into());
        self.0.scores.borrow_mut().push(score);
        self.0.reasons.borrow_mut().push(reason.into());
        if let Some(a) = self.0.answer.borrow().clone() {
            self.0.delivered.borrow_mut().push((name.into(), address.into(), a));
        }
    }
}

pub struct FakePairingsState {
    pub headsets: RefCell<Vec<BluetoothHeadset>>,
    pub reads: Cell<usize>,
}

pub struct FakePairings(pub Rc<FakePairingsState>);

impl BluetoothInventory for FakePairings {
    fn paired_headsets(&self) -> Vec<BluetoothHeadset> {
        self.0.reads.set(self.0.reads.get() + 1);
        self.0.headsets.borrow().clone()
    }
}

/// The writer log as a script (Swift `FakeVolumeWriterSource`).
#[derive(Default)]
pub struct FakeWriterState {
    pub start_result: Cell<bool>,
    pub made: Cell<usize>,
    pub starts: Cell<usize>,
}

pub struct FakeWriterSource(pub Rc<FakeWriterState>);

impl cleat_rs::outvol::writer_log::VolumeWriterSource for FakeWriterSource {
    fn start(&mut self) -> bool {
        self.0.starts.set(self.0.starts.get() + 1);
        self.0.start_result.get()
    }
    fn stop(&mut self) {}
}

pub struct Opts {
    pub writer_starts: bool,
    pub routing_available: bool,
    pub route_answer: Option<RouteResponse>,
    pub headsets: Vec<BluetoothHeadset>,
    pub start_results: Vec<bool>,
    pub mode: Mode,
    pub trace: bool,
    pub identity: Identity,
    pub agent_status: AgentStatus,
    pub microphone: MicrophonePermission,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            writer_starts: true,
            routing_available: true,
            route_answer: None,
            headsets: vec![],
            start_results: vec![],
            mode: Mode::Enforce,
            trace: false,
            identity: Identity::default(),
            agent_status: AgentStatus::NotRegistered,
            microphone: MicrophonePermission::Granted,
        }
    }
}

pub fn release_identity() -> Identity {
    Identity { bundle_id: Some("ai.jetto.cleat".into()), version: None }
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct Harness {
    pub dir: PathBuf,
    pub audio: Rc<FakeAudioState>,
    pub clock: FakeClock,
    pub agent: Rc<FakeAgentState>,
    pub login_item: Rc<FakeAgentState>,
    pub reports: Rc<RefCell<Vec<bool>>>,
    pub detectors: Rc<DetectorLog>,
    pub routing: Rc<FakeRoutingState>,
    pub pairings: Rc<FakePairingsState>,
    pub writer: Rc<FakeWriterState>,
    pub engine: Engine,
}

impl Harness {
    pub fn new(config: &Config, snapshot: DeviceSnapshot, opts: Opts) -> Self {
        Self::with_json(&serde_json::to_string(config).unwrap(), snapshot, opts)
    }

    /// Writes the config file and runs `start` (load, side effects, baseline pass).
    pub fn with_json(config_json: &str, snapshot: DeviceSnapshot, opts: Opts) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("cleat-rs-engine-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), config_json).unwrap();
        let audio = Rc::new(FakeAudioState {
            snapshot: RefCell::new(snapshot),
            writes: RefCell::default(),
            output_writes_stick: Cell::new(true),
            owns: RefCell::default(),
        });
        let clock = FakeClock::default();
        let agent = FakeAgentState::new(opts.agent_status);
        let login_item = FakeAgentState::new(AgentStatus::NotRegistered);
        let reports = Rc::new(RefCell::new(vec![]));
        let sink = reports.clone();
        let detectors = Rc::new(DetectorLog { start_results: opts.start_results, ..Default::default() });
        let log = detectors.clone();
        let routing = Rc::new(FakeRoutingState {
            available: opts.routing_available,
            answer: RefCell::new(opts.route_answer),
            addresses: RefCell::default(),
            scores: RefCell::default(),
            reasons: RefCell::default(),
            delivered: RefCell::default(),
        });
        let pairings = Rc::new(FakePairingsState { headsets: RefCell::new(opts.headsets), reads: Cell::new(0) });
        let writer = Rc::new(FakeWriterState::default());
        writer.start_result.set(opts.writer_starts);
        let w = writer.clone();
        let deps = EngineDeps {
            system: Box::new(FakeAudio(audio.clone())),
            log: EventLog::new(dir.join("cleat.log"), dir.join("cleat.log.1")),
            config_path: dir.join("config.json"),
            status_path: dir.join("status.json"),
            clock: Box::new(clock.clone()),
            identity: opts.identity,
            agent: Box::new(FakeAgent(agent.clone())),
            login_item: Box::new(FakeAgent(login_item.clone())),
            launchd: Box::new(FakeLaunchd),
            error_reports_changed: Box::new(move |v| sink.borrow_mut().push(v)),
            detectors: Box::new(move |device, _rate, zero_seconds| {
                log.made.set(log.made.get() + 1);
                Box::new(FakeDetector {
                    device_id: device.id,
                    name: device.name.clone(),
                    zero_seconds,
                    log: log.clone(),
                })
            }),
            routing: Box::new(FakeRouting(routing.clone())),
            inventory: Box::new(FakePairings(pairings.clone())),
            writer_source: Box::new(move || {
                w.made.set(w.made.get() + 1);
                Box::new(FakeWriterSource(w.clone()))
            }),
            events: None,
        };
        let mut engine = Engine::new(deps, opts.mode, opts.trace);
        engine.start(opts.microphone);
        let mut h = Self { dir, audio, clock, agent, login_item, reports, detectors, routing, pairings, writer, engine };
        h.deliver_answers();
        h
    }

    /// Hands queued routing answers to the engine, as the GCD hop would.
    pub fn deliver_answers(&mut self) {
        loop {
            let next = {
                let mut q = self.routing.delivered.borrow_mut();
                if q.is_empty() { None } else { Some(q.remove(0)) }
            };
            let Some((name, address, response)) = next else { break };
            self.engine.handle(Event::RouteAnswered { name, address, response });
        }
    }

    /// One pass, answers delivered (Swift `world.reconcile()`).
    pub fn reconcile(&mut self) {
        self.engine.reconcile(false, None);
        self.deliver_answers();
    }

    /// Walks the clock forward without firing timers (Swift `world.advance`).
    pub fn advance(&self, d: Duration) {
        self.clock.0.set(self.clock.0.get() + d);
    }

    pub fn requests(&self) -> Vec<String> {
        self.routing.addresses.borrow().clone()
    }

    pub fn count(&self, needle: &str) -> usize {
        self.log_lines().iter().filter(|l| l.contains(needle)).count()
    }

    pub fn writes(&self) -> Vec<String> {
        self.audio.writes.borrow().clone()
    }

    pub fn set(&self, f: impl FnOnce(&mut DeviceSnapshot)) {
        f(&mut self.audio.snapshot.borrow_mut());
    }

    pub fn log_lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("cleat.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    pub fn status(&self) -> Status {
        Status::read(&self.dir.join("status.json")).expect("status.json")
    }

    pub fn rule(&self, name: &str) -> String {
        self.status().rules.get(name).cloned().unwrap_or_default()
    }

    /// What a CoreAudio listener would deliver.
    pub fn fire(&mut self, kind: ListenerKind) {
        self.engine.handle(Event::Listener { kind, received: SystemTime::now() });
    }

    /// Advances the clock by `ms`, firing every timer that falls due on the way, in order.
    pub fn drive(&mut self, ms: u64) {
        let target = self.clock.0.get() + Duration::from_millis(ms);
        while let Some(d) = self.engine.next_deadline() {
            if d > target {
                break;
            }
            self.clock.0.set(d.max(self.clock.0.get()));
            self.engine.run_due();
            self.deliver_answers();
        }
        self.clock.0.set(target);
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
