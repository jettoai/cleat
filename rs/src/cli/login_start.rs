//! `cleat login-start`, what the login plist runs (B-2739). The SMAppService registration can be
//! lost, and then launchd has no job to start at login. This registers the agent again and kicks
//! it. It never opens the app: that would open the settings window over a daemon already running.

use std::path::Path;

use crate::config::{paths, Config};
use crate::identity::Identity;
use crate::launch::{failure_detail, needs_reregistration, AgentService, LaunchctlJob, Launchd, SmApp};
use crate::state::EventLog;

/// Always 0: a login must not report a failure to launchd. A failure goes to stderr and the log.
pub(super) fn login_start() -> i32 {
    let identity = Identity::current();
    let label = identity.label();
    let agent = SmApp::agent(&format!("{label}.plist"));
    let launchd = LaunchctlJob { label };
    if let Err(e) = start_at_login(&identity, &paths::config_path(), &agent, &launchd) {
        eprintln!("cleat login-start: {e}");
        EventLog::new(paths::log_path(), paths::rotated_log_path()).append(&format!("launchAtLogin: login-start failed ({e})"));
    }
    0
}

/// Nothing to do for a development build, for `launchAtLogin` off (the same reading as
/// `cleat restart`: only a config that loads and says so) or for a job launchd already has. Else the
/// agent is registered (again, if it is registered but has no job) and kickstarted, without `-k`.
pub fn start_at_login(
    identity: &Identity,
    config_path: &Path,
    agent: &dyn AgentService,
    launchd: &dyn Launchd,
) -> Result<(), String> {
    if identity.is_development_build() || Config::load(config_path).is_ok_and(|c| !c.launch_at_login) {
        return Ok(());
    }
    if launchd.loaded_job().0 {
        return Ok(());
    }
    // Enabled with no job is the Homebrew-upgrade leftover `needs_reregistration` names.
    if needs_reregistration(agent.status(), || false) {
        agent.unregister()?;
    }
    agent.register()?;
    let result = launchd.kickstart();
    if result.0 != 0 {
        return Err(format!("kickstart failed ({})", failure_detail(&result)));
    }
    Ok(())
}
