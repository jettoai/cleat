//! The switch on the General page turned back on from the same window (B-1316): the daemon left with
//! the agent when it went off, so the window registers the agent itself, after the file is written.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cleat_rs::launch::{AgentService, AgentStatus};
use cleat_rs::settings::store::{Phase, Store};

#[derive(Default)]
struct Seen {
    registers: Cell<usize>,
    /// The file as it was when `register` ran.
    file_at_register: RefCell<Option<String>>,
}

struct FakeAgent {
    status: AgentStatus,
    path: PathBuf,
    seen: Rc<Seen>,
}

impl AgentService for FakeAgent {
    fn status(&self) -> AgentStatus {
        self.status
    }
    fn register(&self) -> Result<(), String> {
        self.seen.registers.set(self.seen.registers.get() + 1);
        *self.seen.file_at_register.borrow_mut() = std::fs::read_to_string(&self.path).ok();
        Ok(())
    }
    fn unregister(&self) -> Result<(), String> {
        Ok(())
    }
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cleat-rs-store-launch-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A window loaded on a file with the switch off, with `agent` (or a development build when `None`).
fn window(d: &Path, agent: Option<AgentStatus>) -> (Store, Rc<Seen>) {
    let path = d.join("config.json");
    std::fs::write(&path, r#"{"launchAtLogin": false}"#).unwrap();
    let seen = Rc::new(Seen::default());
    let agent = agent.map(|status| Box::new(FakeAgent { status, path: path.clone(), seen: seen.clone() }) as Box<dyn AgentService>);
    let mut store = Store::with_paths(path, d.join("status.json"), agent);
    assert!(store.begin_load());
    store.apply_paired(&[]);
    assert_eq!(store.phase, Phase::Ready);
    assert!(!store.draft.launch_at_login);
    (store, seen)
}

#[test]
fn switching_back_on_registers_after_the_write() {
    let d = dir("on");
    let (mut store, seen) = window(&d, Some(AgentStatus::NotRegistered));
    store.draft.launch_at_login = true;
    store.save_now();
    assert_eq!(store.error_message, None);
    assert_eq!(seen.registers.get(), 1);
    let at_register = seen.file_at_register.borrow().clone().unwrap();
    assert!(!at_register.contains("launchAtLogin"), "registered before the write: {at_register}");
}

#[test]
fn an_enabled_agent_is_not_registered_again() {
    let (mut store, seen) = window(&dir("enabled"), Some(AgentStatus::Enabled));
    store.draft.launch_at_login = true;
    store.save_now();
    assert_eq!(seen.registers.get(), 0);
}

#[test]
fn switched_off_registers_nothing() {
    let (mut store, seen) = window(&dir("off"), Some(AgentStatus::NotRegistered));
    store.draft.balance_enabled = true;
    store.save_now();
    assert_eq!(store.error_message, None);
    assert_eq!(seen.registers.get(), 0);
}

#[test]
fn a_development_build_saves_without_an_agent() {
    let d = dir("dev");
    let (mut store, _) = window(&d, None);
    store.draft.launch_at_login = true;
    store.save_now();
    assert_eq!(store.error_message, None);
    assert!(!std::fs::read_to_string(d.join("config.json")).unwrap().contains("launchAtLogin"));
}

#[test]
fn a_rejected_save_registers_nothing() {
    let d = dir("conflict");
    let (mut store, seen) = window(&d, Some(AgentStatus::NotRegistered));
    std::fs::write(d.join("config.json"), r#"{"launchAtLogin": false, "zeta": 1}"#).unwrap();
    store.draft.launch_at_login = true;
    store.save_now();
    assert!(store.has_conflict);
    assert_eq!(seen.registers.get(), 0);
}
