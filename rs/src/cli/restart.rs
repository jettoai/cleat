//! `cleat restart` (Swift `CLI.restart`): start or replace the daemon through the launchd agent,
//! registering it first when that is what is missing. `kickstart -k` kills the running copy.

use super::fail;
use crate::config::{paths, Config};
use crate::identity::Identity;
use crate::launch::{
    domain_target, failure_detail, is_registered, launchctl, needs_reregistration, AgentService, AgentStatus,
    LaunchctlJob, Launchd, SmApp,
};

pub(super) fn restart() -> i32 {
    let identity = Identity::current();
    let label = identity.label();
    let service = SmApp::agent(&format!("{label}.plist"));
    let config = paths::tilde(&paths::config_path());
    if !identity.is_development_build() {
        if launch_at_login_is_off() {
            return fail(&format!(
                "restart: \"launchAtLogin\" is off in {config}; launchd keeps Cleat running only through its agent, set it to true"
            ));
        }
        if !is_registered(service.status()) {
            match service.register() {
                Ok(()) => println!("registered {label}"),
                Err(e) => return fail(&format!("restart: could not register {label} ({e})")),
            }
        }
        let job = LaunchctlJob { label: label.clone() };
        if needs_reregistration(service.status(), || job.loaded_job().0) {
            match service.unregister().and_then(|_| service.register()) {
                Ok(()) => println!("re-registered {label} (it was registered but launchd had no job)"),
                Err(e) => return fail(&format!("restart: could not re-register {label} ({e})")),
            }
        }
    }

    let target = domain_target(&label);
    let result = launchctl(&["kickstart", "-k", &target]);
    if result.0 != 0 {
        let next = next_step(&identity, service.status(), &config);
        return fail(&format!(
            "restart: launchd has no {label} loaded ({})\n         {next}",
            failure_detail(&result)
        ));
    }
    println!("restarted {target}");
    0
}

fn next_step(identity: &Identity, status: AgentStatus, config: &str) -> String {
    if identity.is_development_build() {
        return "this is the dev build, which never registers itself - load a plist of your own with launchctl bootstrap"
            .into();
    }
    match status {
        AgentStatus::RequiresApproval => {
            "allow Cleat under System Settings > General > Login Items, then run this again".into()
        }
        _ => format!("set \"launchAtLogin\" to true in {config} - with it off, nothing keeps Cleat running"),
    }
}

/// Only a config that loads and says so blocks a restart.
fn launch_at_login_is_off() -> bool {
    Config::load(&paths::config_path()).is_ok_and(|c| !c.launch_at_login)
}
