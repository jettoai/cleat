//! Proof that observe mode decides but never reaches a CoreAudio writer, with an enforce-mode
//! control showing the same fixture does reach the writer.

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cleat_rs::audio::{AudioSystem, ListenTarget, ListenerToken};
use cleat_rs::config::Config;
use cleat_rs::engine::{Engine, Mode};
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::state::EventLog;
use common::*;

#[derive(Default)]
struct FakeState {
    snapshot: RefCell<DeviceSnapshot>,
    writes: RefCell<Vec<String>>,
}

struct FakeAudio(Rc<FakeState>);

impl AudioSystem for FakeAudio {
    fn snapshot(&self, _config: &Config) -> DeviceSnapshot {
        self.0.snapshot.borrow().clone()
    }
    fn set_default_input(&self, id: u32) -> i32 {
        self.0.writes.borrow_mut().push(format!("setDefaultInput({id})"));
        0
    }
    fn set_default_output(&self, id: u32) -> i32 {
        self.0.writes.borrow_mut().push(format!("setDefaultOutput({id})"));
        0
    }
    fn set_balance(&self, id: u32, value: f32) -> i32 {
        self.0.writes.borrow_mut().push(format!("setBalance({id}, {value})"));
        0
    }
    fn set_input_volume(&self, id: u32, value: f32) -> i32 {
        self.0.writes.borrow_mut().push(format!("setInputVolume({id}, {value:.1})"));
        0
    }
    fn add_listener(&self, _target: ListenTarget) -> Option<ListenerToken> {
        None
    }
    fn remove_listener(&self, _token: ListenerToken) {}
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("cleat-rs-test-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `start()` (which reads the config file and runs the baseline pass) against a fake.
fn run(mode: Mode, config_json: &str, snapshot: DeviceSnapshot) -> (Vec<String>, String) {
    let dir = temp_dir();
    let config = dir.join("config.json");
    std::fs::write(&config, config_json).unwrap();
    let state = Rc::new(FakeState { snapshot: RefCell::new(snapshot), ..Default::default() });
    let log = EventLog::new(dir.join("cleat-rs.log"), dir.join("cleat-rs.log.1"));
    let mut engine = Engine::new(Box::new(FakeAudio(state.clone())), mode, log, config, dir.join("status.json"));
    engine.start(None);
    let text = std::fs::read_to_string(dir.join("cleat-rs.log")).unwrap();
    let writes = state.writes.borrow().clone();
    let _ = std::fs::remove_dir_all(&dir);
    (writes, text)
}

const FOUR_RULES: &str = r#"{
  "input": ["Wireless microphone", "Brio 100"],
  "output": ["Studio Display Speakers"],
  "blockedOutput": ["MacBook Pro Speakers"],
  "inputVolume": {"*": 100}
}"#;

fn four_rules_snapshot() -> DeviceSnapshot {
    DeviceSnapshot {
        devices: vec![wireless(), brio(), display_speakers(), mac_speakers()],
        default_input: Some(brio().id),
        default_output: Some(mac_speakers().id),
        input_volumes: [(wireless().id, 0.5f32)].into_iter().collect(),
        ..Default::default()
    }
}

#[test]
fn observe_decides_but_never_writes() {
    let (writes, log) = run(Mode::Observe, FOUR_RULES, four_rules_snapshot());
    assert_eq!(writes.len(), 0, "observe mode reached a writer: {writes:?}");
    assert!(log.contains("pinInput: Brio 100 -> Wireless microphone (higher priority present) [observe: not applied]"));
    assert!(log.contains("pinOutput: MacBook Pro Speakers -> Studio Display Speakers (blocked) [observe: not applied]"));
    assert!(log.contains("inputVolume: Wireless microphone 50% -> 100% [observe: not applied]"));
    assert!(log.contains("actions=3"));
}

#[test]
fn observe_never_writes_balance() {
    let snapshot = DeviceSnapshot {
        devices: vec![display_speakers()],
        default_output: Some(display_speakers().id),
        output_balance: Some(0.3),
        ..Default::default()
    };
    let (writes, log) = run(Mode::Observe, r#"{"balance": 0.5}"#, snapshot);
    assert_eq!(writes.len(), 0, "observe mode reached a writer: {writes:?}");
    assert!(log.contains("balance: Studio Display Speakers 0.30 -> 0.50 [observe: not applied]"));
}

#[test]
fn enforce_mode_reaches_the_writer() {
    let (writes, log) = run(Mode::Enforce, FOUR_RULES, four_rules_snapshot());
    assert_eq!(writes, vec!["setDefaultInput(10)", "setDefaultOutput(60)", "setInputVolume(10, 1.0)"]);
    assert!(!log.contains("[observe: not applied]"));
    assert!(log.contains("pinInput: Brio 100 -> Wireless microphone (higher priority present)"));
}

#[test]
fn nothing_to_do_still_writes_a_pass_line() {
    let (writes, log) = run(Mode::Observe, "{}", four_rules_snapshot());
    assert!(writes.is_empty());
    assert!(log.contains("pass: trigger=startup"));
    assert!(log.contains("actions=0 (nothing to do)"));
}
