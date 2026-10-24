//! `cleat login-start` (B-2739), the command the login plist runs: what it asks of the agent and
//! launchd in each situation. Nothing here may open the app or start the daemon itself.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cleat_rs::cli::start_at_login;
use cleat_rs::identity::Identity;
use cleat_rs::launch::{AgentService, AgentStatus, Launchd};

type Calls = Rc<RefCell<Vec<&'static str>>>;

struct Agent {
    status: AgentStatus,
    register: Result<(), String>,
    calls: Calls,
}

impl AgentService for Agent {
    fn status(&self) -> AgentStatus {
        self.status
    }
    fn register(&self) -> Result<(), String> {
        self.calls.borrow_mut().push("register");
        self.register.clone()
    }
    fn unregister(&self) -> Result<(), String> {
        self.calls.borrow_mut().push("unregister");
        Ok(())
    }
}

struct Job {
    loaded: bool,
    kickstart: i32,
    calls: Calls,
}

impl Launchd for Job {
    fn loaded_job(&self) -> (bool, Option<i32>) {
        (self.loaded, None)
    }
    fn kickstart(&self) -> (i32, String) {
        self.calls.borrow_mut().push("kickstart");
        (self.kickstart, String::new())
    }
    fn started_by_launchd(&self) -> bool {
        self.calls.borrow_mut().push("started_by_launchd");
        false
    }
    fn exit(&self) {
        self.calls.borrow_mut().push("exit");
    }
}

fn release() -> Identity {
    Identity { bundle_id: Some("ai.jetto.cleat".into()), version: None }
}

fn config(tag: &str, json: Option<&str>) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cleat-rs-login-start-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("config.json");
    if let Some(json) = json {
        std::fs::write(&p, json).unwrap();
    }
    p
}

struct Case {
    identity: Identity,
    config: PathBuf,
    status: AgentStatus,
    register: Result<(), String>,
    loaded: bool,
    kickstart: i32,
}

fn case(tag: &str) -> Case {
    Case {
        identity: release(),
        config: config(tag, None),
        status: AgentStatus::NotRegistered,
        register: Ok(()),
        loaded: false,
        kickstart: 0,
    }
}

fn run(c: &Case) -> (Result<(), String>, Vec<&'static str>) {
    let calls = Calls::default();
    let agent = Agent { status: c.status, register: c.register.clone(), calls: calls.clone() };
    let job = Job { loaded: c.loaded, kickstart: c.kickstart, calls: calls.clone() };
    let result = start_at_login(&c.identity, Path::new(&c.config), &agent, &job);
    let seen = calls.borrow().clone();
    (result, seen)
}

#[test]
fn a_job_launchd_already_has_is_left_alone() {
    let (result, calls) = run(&Case { loaded: true, ..case("loaded") });
    assert_eq!((result, calls), (Ok(()), vec![]));
}

#[test]
fn a_lost_registration_is_registered_and_kicked_without_a_restart() {
    let (result, calls) = run(&case("lost"));
    assert_eq!((result, calls), (Ok(()), vec!["register", "kickstart"]));
}

#[test]
fn enabled_without_a_job_is_registered_again_then_kicked() {
    let (result, calls) = run(&Case { status: AgentStatus::Enabled, ..case("enabled") });
    assert_eq!((result, calls), (Ok(()), vec!["unregister", "register", "kickstart"]));
}

#[test]
fn a_pending_approval_is_registered_again_and_kicked() {
    let (_, calls) = run(&Case { status: AgentStatus::RequiresApproval, ..case("approval") });
    assert_eq!(calls, vec!["register", "kickstart"]);
}

#[test]
fn launch_at_login_off_does_nothing() {
    let off = config("off", Some(r#"{"launchAtLogin": false}"#));
    let (result, calls) = run(&Case { config: off, ..case("off") });
    assert_eq!((result, calls), (Ok(()), vec![]));
}

#[test]
fn a_missing_or_unreadable_config_counts_as_on() {
    for (tag, json) in [("missing", None), ("invalid", Some(r#"{"balance": 2}"#))] {
        let (_, calls) = run(&Case { config: config(tag, json), ..case(tag) });
        assert_eq!(calls, vec!["register", "kickstart"], "{tag}");
    }
}

#[test]
fn a_failed_register_is_reported_and_nothing_is_kicked() {
    let (result, calls) = run(&Case { register: Err("denied".into()), ..case("fail") });
    assert_eq!(result, Err("denied".into()));
    assert_eq!(calls, vec!["register"]);
}

#[test]
fn a_failed_kickstart_is_reported() {
    let (result, calls) = run(&Case { kickstart: 113, ..case("kick") });
    assert!(result.unwrap_err().contains("kickstart failed"));
    assert_eq!(calls, vec!["register", "kickstart"]);
}

#[test]
fn development_builds_do_nothing() {
    for bundle in [None, Some("ai.jetto.cleat.dev"), Some("ai.jetto.cleat.rs")] {
        let identity = Identity { bundle_id: bundle.map(String::from), version: None };
        let (result, calls) = run(&Case { identity, ..case("dev") });
        assert_eq!((result, calls), (Ok(()), vec![]), "{bundle:?}");
    }
}

#[test]
fn it_never_hands_over_exits_or_asks_who_started_it() {
    for status in [AgentStatus::NotRegistered, AgentStatus::Enabled, AgentStatus::RequiresApproval, AgentStatus::NotFound] {
        let (_, calls) = run(&Case { status, ..case("never") });
        assert!(calls.iter().all(|c| ["register", "unregister", "kickstart"].contains(c)), "{status:?}: {calls:?}");
    }
}
