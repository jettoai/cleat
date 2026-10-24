//! `launchAtLogin` sync (Swift `Engine+LaunchAgent.swift`) against a fake SMAppService.

mod common;

use cleat_rs::config::Config;
use cleat_rs::identity::Identity;
use cleat_rs::launch::AgentStatus;
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::settings::draft::{LiveLevels, SettingsDraft};
use common::engine::{release_identity, Harness, Opts, FAKE_EXE};

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

// B-2739: the plain launchd agent that starts the app at login without SMAppService.

/// A config change that keeps launchAtLogin on, so the sync runs again.
fn touch_config(h: &mut Harness) {
    std::fs::write(h.dir.join("config.json"), r#"{"usagePing": false}"#).unwrap();
    reload(h);
}

fn plist_text(h: &Harness) -> String {
    std::fs::read_to_string(h.login_plist()).unwrap()
}

fn no_launch_at_login() -> String {
    serde_json::to_string(&Config { launch_at_login: false, ..Config::default() }).unwrap()
}

#[test]
fn the_login_plist_is_written_when_wanted() {
    let h = start("{}", release_identity(), AgentStatus::NotRegistered);
    let text = plist_text(&h);
    assert!(text.contains("<string>ai.jetto.cleat.login</string>"));
    // The executable and `login-start`, no `open`: that would pop the settings window.
    assert!(text.contains(&format!("<string>{FAKE_EXE}</string>\n\t\t<string>login-start</string>\n\t</array>")));
    assert!(!text.contains("/usr/bin/open"));
    assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"));
    assert!(text.contains("<string>Interactive</string>"));
    assert!(!text.contains("KeepAlive"));
    assert_eq!(h.count("launchAtLogin: login plist written"), 1);
}

#[test]
fn an_identical_login_plist_is_left_alone_and_silent() {
    let mut h = start("{}", release_identity(), AgentStatus::NotRegistered);
    let before = std::fs::metadata(h.login_plist()).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    // A config change that keeps launchAtLogin on runs the sync again.
    touch_config(&mut h);
    assert!(logged(&h, "config: reloaded (ok)"));
    assert_eq!(std::fs::metadata(h.login_plist()).unwrap().modified().unwrap(), before);
    assert_eq!(h.count("login plist"), 1, "only the first write was logged");
}

#[test]
fn a_different_login_plist_is_overwritten() {
    let mut h = start("{}", release_identity(), AgentStatus::NotRegistered);
    std::fs::write(h.login_plist(), "stale").unwrap();
    touch_config(&mut h);
    assert!(plist_text(&h).contains(FAKE_EXE));
    assert_eq!(h.count("launchAtLogin: login plist written"), 2);
}

#[test]
fn switching_launch_at_login_off_removes_the_login_plist() {
    let mut h = fresh(AgentStatus::NotRegistered, true);
    assert!(h.login_plist().exists());
    save_from_window(&mut h, false);
    assert!(!h.login_plist().exists());
    assert!(logged(&h, "launchAtLogin: login plist removed"));
}

#[test]
fn removing_a_login_plist_that_is_not_there_is_silent() {
    let h = start(&no_launch_at_login(), release_identity(), AgentStatus::NotRegistered);
    assert!(!h.login_plist().exists());
    assert_eq!(h.count("login plist"), 0);
}

#[test]
fn a_login_plist_that_cannot_be_written_is_only_logged() {
    let h = Harness::with_json(
        "{}",
        DeviceSnapshot::default(),
        Opts { identity: release_identity(), login_plist_blocked: true, ..Opts::default() },
    );
    assert_eq!(h.count("launchAtLogin: login plist write failed ("), 1);
    // The rest of the sync went on.
    assert_eq!(h.agent.registers.get(), 1);
}

#[test]
fn development_builds_never_touch_the_login_plist() {
    for bundle in ["ai.jetto.cleat.rs", "ai.jetto.cleat.dev"] {
        let identity = Identity { bundle_id: Some(bundle.into()), version: None };
        let h = start("{}", identity, AgentStatus::NotRegistered);
        assert!(!h.dir.join("LaunchAgents").exists(), "{bundle}");
        assert_eq!(h.count("login plist"), 0, "{bundle}");
    }
}

#[test]
fn a_config_that_did_not_load_leaves_the_login_plist_alone() {
    let h = start(r#"{"balance": 2}"#, release_identity(), AgentStatus::NotRegistered);
    assert!(!h.dir.join("LaunchAgents").exists());
}

#[test]
fn a_path_with_markup_characters_stays_one_escaped_argument() {
    let text = cleat_rs::launch::login_plist::contents(std::path::Path::new("/A & B <x>.app/Contents/MacOS/Cleat"));
    assert!(text.contains("<string>/A &amp; B &lt;x&gt;.app/Contents/MacOS/Cleat</string>"));
    assert!(!text.contains("B <x>"));
}

#[test]
fn a_login_plist_from_the_first_version_is_replaced_on_the_next_start() {
    let mut h = start("{}", release_identity(), AgentStatus::NotRegistered);
    let first_version = plist_text(&h)
        .replace(&format!("<string>{FAKE_EXE}</string>\n\t\t<string>login-start</string>"), "<string>/usr/bin/open</string>\n\t\t<string>-g</string>");
    assert!(first_version.contains("/usr/bin/open"));
    std::fs::write(h.login_plist(), &first_version).unwrap();
    touch_config(&mut h);
    let text = plist_text(&h);
    assert!(text.contains("<string>login-start</string>") && !text.contains("/usr/bin/open"));
    assert_eq!(h.count("launchAtLogin: login plist written"), 2);
}
