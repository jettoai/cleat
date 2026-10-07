//! `launchAtLogin` sync (Swift `Engine+LaunchAgent.swift`) against a fake SMAppService.

mod common;

use cleat_rs::config::Config;
use cleat_rs::identity::Identity;
use cleat_rs::launch::AgentStatus;
use cleat_rs::model::DeviceSnapshot;
use common::engine::{release_identity, Harness, Opts};

fn start(config_json: &str, identity: Identity, agent: AgentStatus) -> Harness {
    Harness::with_json(
        config_json,
        DeviceSnapshot::default(),
        Opts { identity, agent_status: agent, ..Opts::default() },
    )
}

#[test]
fn development_builds_never_register() {
    for bundle in [None, Some("ai.jetto.cleat.rs"), Some("ai.jetto.cleat.dev")] {
        let identity = Identity { bundle_id: bundle.map(String::from), version: None };
        let h = start("{}", identity, AgentStatus::NotRegistered);
        assert_eq!(h.agent.registers.get(), 0, "{bundle:?}");
        assert_eq!(h.agent.status_reads.get(), 0, "{bundle:?}");
        assert_eq!(h.login_item.status_reads.get(), 0, "{bundle:?}");
    }
}

#[test]
fn release_build_registers_when_wanted() {
    let h = start("{}", release_identity(), AgentStatus::NotRegistered);
    assert_eq!(h.agent.registers.get(), 1);
    assert!(h.log_lines().iter().any(|l| l.ends_with("launchAtLogin: launchd agent registered")));
}

#[test]
fn release_build_unregisters_when_not_wanted() {
    let config = serde_json::to_string(&Config { launch_at_login: false, ..Config::default() }).unwrap();
    let h = start(&config, release_identity(), AgentStatus::Enabled);
    assert_eq!(h.agent.unregisters.get(), 1);
    assert_eq!(h.agent.registers.get(), 0);
    assert!(h.log_lines().iter().any(|l| l.ends_with("launchAtLogin: launchd agent unregistered")));
}

#[test]
fn a_config_that_did_not_load_touches_nothing() {
    let h = start(r#"{"balance": 2}"#, release_identity(), AgentStatus::Enabled);
    assert_eq!(h.agent.registers.get() + h.agent.unregisters.get(), 0);
    assert_eq!(h.agent.status_reads.get(), 0);
}

#[test]
fn enabled_without_a_loaded_job_is_registered_again() {
    let h = start("{}", release_identity(), AgentStatus::Enabled);
    // The job is not loaded (FakeLaunchd), so an enabled registration is made again.
    assert_eq!(h.agent.unregisters.get(), 1);
    assert_eq!(h.agent.registers.get(), 1);
    assert!(h
        .log_lines()
        .iter()
        .any(|l| l.ends_with("launchAtLogin: launchd agent re-registered (registered but not loaded)")));
}
