//! A window that opens with the switch on while the system has lost the registration (B-2739):
//! the agent is registered again, and what stays unregistered is said under the switch.

use std::cell::Cell;
use std::rc::Rc;

use cleat_rs::launch::{AgentService, AgentStatus};
use cleat_rs::settings::store::{Phase, Store};

struct Fake {
    status: Rc<Cell<AgentStatus>>,
    registers: Rc<Cell<usize>>,
    /// What `register` does: Ok moves the status here, Err leaves it.
    register_to: Result<AgentStatus, String>,
}

impl AgentService for Fake {
    fn status(&self) -> AgentStatus {
        self.status.get()
    }
    fn register(&self) -> Result<(), String> {
        self.registers.set(self.registers.get() + 1);
        self.register_to.clone().map(|s| self.status.set(s))
    }
    fn unregister(&self) -> Result<(), String> {
        Ok(())
    }
}

/// A window opened on `config` with an agent in `start`; returns the store and the register count.
fn open(tag: &str, config: &str, start: AgentStatus, register_to: Result<AgentStatus, String>) -> (Store, Rc<Cell<usize>>) {
    let d = std::env::temp_dir().join(format!("cleat-rs-launch-check-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("config.json"), config).unwrap();
    let registers = Rc::new(Cell::new(0));
    let agent = Fake { status: Rc::new(Cell::new(start)), registers: registers.clone(), register_to };
    let mut store = Store::with_paths(d.join("config.json"), d.join("status.json"), Some(Box::new(agent)));
    assert!(store.begin_load());
    store.apply_paired(&[]);
    assert_eq!(store.phase, Phase::Ready);
    (store, registers)
}

const ON: &str = r#"{"launchAtLogin": true}"#;

#[test]
fn a_lost_registration_is_made_again_and_nothing_is_shown() {
    let (store, registers) = open("lost", ON, AgentStatus::NotRegistered, Ok(AgentStatus::Enabled));
    assert_eq!(registers.get(), 1);
    assert_eq!(store.launch_warning, None);
}

#[test]
fn a_failed_registration_is_shown_with_its_reason() {
    let (store, registers) = open("fail", ON, AgentStatus::NotRegistered, Err("Operation not permitted".into()));
    assert_eq!(registers.get(), 1);
    let w = store.launch_warning.expect("a warning");
    assert!(w.contains("Operation not permitted"), "{w}");
}

#[test]
fn an_approval_that_is_still_pending_points_to_login_items() {
    let (store, registers) = open("approval", ON, AgentStatus::NotRegistered, Ok(AgentStatus::RequiresApproval));
    assert_eq!(registers.get(), 1);
    let w = store.launch_warning.expect("a warning");
    assert!(w.contains("Login Items") || w.contains("登入項目"), "{w}");
}

#[test]
fn a_registration_the_system_cannot_find_is_shown() {
    for (tag, after) in [("notfound", AgentStatus::NotFound), ("still", AgentStatus::NotRegistered)] {
        let (store, _) = open(tag, ON, AgentStatus::NotRegistered, Ok(after));
        assert!(store.launch_warning.is_some(), "{after:?}");
    }
}

#[test]
fn an_enabled_agent_is_left_alone() {
    let (store, registers) = open("enabled", ON, AgentStatus::Enabled, Ok(AgentStatus::Enabled));
    assert_eq!(registers.get(), 0);
    assert_eq!(store.launch_warning, None);
}

#[test]
fn a_switch_that_is_off_does_nothing() {
    let (store, registers) = open("off", r#"{"launchAtLogin": false}"#, AgentStatus::NotRegistered, Ok(AgentStatus::Enabled));
    assert_eq!(registers.get(), 0);
    assert_eq!(store.launch_warning, None);
}

#[test]
fn a_missing_config_counts_as_on() {
    let (store, registers) = open("missing", "{}", AgentStatus::NotRegistered, Ok(AgentStatus::Enabled));
    assert_eq!(registers.get(), 1);
    assert_eq!(store.launch_warning, None);
}
