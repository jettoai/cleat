//! Engine doubles (Swift `EngineTestDoubles.swift`) and a harness that drives a real `Engine` on a
//! fake clock: `drive(ms)` is Swift's `waitOnQueue`, without the wait.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use cleat_rs::audio::{AudioSystem, ListenTarget, ListenerKind, ListenerToken};
use cleat_rs::config::Config;
use cleat_rs::engine::{Engine, EngineDeps, Event, Mode, Status};
use cleat_rs::identity::Identity;
use cleat_rs::launch::{AgentService, AgentStatus, Launchd};
use cleat_rs::model::{DeviceSnapshot, MicrophonePermission};
use cleat_rs::state::clock::Clock;
use cleat_rs::state::EventLog;

/// The audio system as a value. Writes land back in the snapshot, so a second pass converges.
pub struct FakeAudioState {
    pub snapshot: RefCell<DeviceSnapshot>,
    pub writes: RefCell<Vec<String>>,
    /// False is a device that has just appeared: `setDefaultOutput` succeeds and does not stick.
    pub output_writes_stick: Cell<bool>,
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
    fn add_listener(&self, _target: ListenTarget) -> Option<ListenerToken> {
        None
    }
    fn remove_listener(&self, _token: ListenerToken) {}
}

#[derive(Clone, Default)]
pub struct FakeClock(pub Rc<Cell<Duration>>);

impl Clock for FakeClock {
    fn mono(&self) -> Duration {
        self.0.get()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::now()
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

pub struct Opts {
    pub mode: Mode,
    pub trace: bool,
    pub identity: Identity,
    pub agent_status: AgentStatus,
    pub microphone: MicrophonePermission,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
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
        });
        let clock = FakeClock::default();
        let agent = FakeAgentState::new(opts.agent_status);
        let login_item = FakeAgentState::new(AgentStatus::NotRegistered);
        let reports = Rc::new(RefCell::new(vec![]));
        let sink = reports.clone();
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
            events: None,
        };
        let mut engine = Engine::new(deps, opts.mode, opts.trace);
        engine.start(opts.microphone);
        Self { dir, audio, clock, agent, login_item, reports, engine }
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
        }
        self.clock.0.set(target);
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
