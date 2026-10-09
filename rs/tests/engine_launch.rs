//! `launchAtLogin` sync (Swift `Engine+LaunchAgent.swift`) against a fake SMAppService.

mod common;

use cleat_rs::config::Config;
use cleat_rs::identity::Identity;
use cleat_rs::launch::AgentStatus;
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::settings::draft::{LiveLevels, SettingsDraft};
use common::engine::{release_identity, Harness, Opts};

fn start(config_json: &str, identity: Identity, agent: AgentStatus) -> Harness {
    Harness::with_json(
        config_json,
        DeviceSnapshot::default(),
        Opts { identity, agent_status: agent, ..Opts::default() },
    )
}

fn fresh(agent: AgentStatus, job_loaded: bool) -> Harness {
    Harness::without_config(
        DeviceSnapshot::default(),
        Opts { identity: release_identity(), agent_status: agent, job_loaded, ..Opts::default() },
    )
}

/// Writes what the settings window writes, then lets the daemon pick it up.
fn save_from_window(h: &mut Harness, launch_at_login: bool) {
    let mut draft = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    draft.launch_at_login = launch_at_login;
    let bytes = cleat_rs::settings::document::merged(None, &draft.managed()).unwrap();
    std::fs::write(h.dir.join("config.json"), bytes).unwrap();
    reload(h);
}

fn reload(h: &mut Harness) {
    h.engine.handle(cleat_rs::engine::Event::ConfigTouched { received: std::time::SystemTime::now() });
    h.drive(400);
}

fn logged(h: &Harness, tail: &str) -> bool {
    h.log_lines().iter().any(|l| l.ends_with(tail))
}

#[test]
fn development_builds_never_register() {
    for bundle in [None, Some("ai.jetto.cleat.rs"), Some("ai.jetto.cleat.dev")] {
        let identity = || Identity { bundle_id: bundle.map(String::from), version: None };
        let with_file = start("{}", identity(), AgentStatus::NotRegistered);
        let no_file = Harness::without_config(
            DeviceSnapshot::default(),
            Opts { identity: identity(), agent_status: AgentStatus::NotRegistered, ..Opts::default() },
        );
        for h in [with_file, no_file] {
            assert_eq!(h.agent.registers.get(), 0, "{bundle:?}");
            assert_eq!(h.agent.status_reads.get(), 0, "{bundle:?}");
            assert_eq!(h.login_item.status_reads.get(), 0, "{bundle:?}");
        }
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
    for agent in [AgentStatus::Enabled, AgentStatus::NotRegistered] {
        let h = start(r#"{"balance": 2}"#, release_identity(), agent);
        assert_eq!(h.agent.registers.get() + h.agent.unregisters.get(), 0, "{agent:?}");
        assert_eq!(h.agent.status_reads.get(), 0, "{agent:?}");
    }
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

#[test]
fn a_missing_config_registers_the_agent() {
    let h = fresh(AgentStatus::NotRegistered, false);
    assert_eq!(h.agent.registers.get(), 1);
    assert!(logged(&h, "launchAtLogin: launchd agent registered"));
}

#[test]
fn a_missing_config_leaves_a_loaded_agent_alone() {
    let h = fresh(AgentStatus::Enabled, true);
    assert_eq!(h.agent.registers.get() + h.agent.unregisters.get(), 0);
}

#[test]
fn the_first_save_from_the_window_keeps_the_agent() {
    let mut h = fresh(AgentStatus::NotRegistered, true);
    assert_eq!(h.agent.registers.get(), 1, "registered on the first launch");
    save_from_window(&mut h, true);
    assert!(logged(&h, "config: reloaded (ok)"), "the save was picked up");
    assert_eq!(h.agent.registers.get(), 1);
    assert_eq!(h.agent.unregisters.get(), 0);
}

#[test]
fn the_window_switch_off_unregisters() {
    let mut h = fresh(AgentStatus::NotRegistered, true);
    save_from_window(&mut h, false);
    assert_eq!(h.agent.unregisters.get(), 1);
    assert!(logged(&h, "launchAtLogin: launchd agent unregistered"));
}

#[test]
fn deleting_the_file_registers_again() {
    let config = serde_json::to_string(&Config { launch_at_login: false, ..Config::default() }).unwrap();
    let mut h = start(&config, release_identity(), AgentStatus::NotRegistered);
    assert_eq!(h.agent.registers.get(), 0);
    std::fs::remove_file(h.dir.join("config.json")).unwrap();
    reload(&mut h);
    assert_eq!(h.agent.registers.get(), 1);
}
