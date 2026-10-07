//! Who keeps the daemon alive (Swift `Engine+LaunchAgent.swift`, branch for branch, log lines word
//! for word).

use super::Engine;
use crate::launch::{domain_target, failure_detail, is_registered, needs_reregistration, AgentStatus};

impl Engine {
    /// `launchAtLogin` registers the launchd agent inside the bundle. Development builds (`.dev`,
    /// `.rs`) never touch any of this.
    pub(super) fn sync_launch_at_login(&mut self) {
        if self.identity.is_development_build() || self.config_state != "ok" {
            return;
        }
        self.retire_login_item();

        let status = self.agent.status();
        let wanted = self.config.launch_at_login;
        if wanted && needs_reregistration(status, || self.launchd.loaded_job().0) {
            let result = self.agent.unregister().and_then(|_| self.agent.register());
            match result {
                Ok(()) => self.note("launchAtLogin: launchd agent re-registered (registered but not loaded)"),
                Err(e) => self.note(&format!("launchAtLogin: re-register failed ({e})")),
            }
            return;
        }
        let needs_change = if wanted { status != AgentStatus::Enabled } else { is_registered(status) };
        if !needs_change {
            return;
        }
        let result = if wanted { self.agent.register() } else { self.agent.unregister() };
        match result {
            Ok(()) => self.note(&format!(
                "launchAtLogin: launchd agent {}",
                if wanted { "registered" } else { "unregistered" }
            )),
            Err(e) => self.note(&format!(
                "launchAtLogin: {} failed ({e})",
                if wanted { "register" } else { "unregister" }
            )),
        }
    }

    /// A daemon LaunchServices started steps aside for the one launchd supervises: either the job
    /// is already up, or a kickstart launchctl reports as successful brings it up.
    pub(super) fn hand_over_to_launch_agent_if_needed(&mut self) {
        if self.launchd.started_by_launchd() {
            return;
        }
        let (loaded, pid) = self.launchd.loaded_job();
        if !loaded || pid == Some(std::process::id() as i32) {
            return;
        }
        if let Some(pid) = pid {
            self.note(&format!("handing over to the launchd agent (pid {pid})"));
            self.launchd.exit();
            return;
        }
        let result = self.launchd.kickstart();
        if result.0 != 0 {
            self.note(&format!(
                "handing over to the launchd agent failed, staying up ({})",
                failure_detail(&result)
            ));
            return;
        }
        self.note("handed over to the launchd agent");
        self.launchd.exit();
    }

    /// Versions up to 0.3.2 registered the app itself as a login item; it goes first.
    fn retire_login_item(&mut self) {
        if !is_registered(self.login_item.status()) {
            return;
        }
        match self.login_item.unregister() {
            Ok(()) => self.note("launchAtLogin: old login item unregistered, the launchd agent replaces it"),
            Err(e) => self.note(&format!("launchAtLogin: unregistering the old login item failed ({e})")),
        }
    }

    /// `gui/<uid>/<label>`, for the CLI and the log.
    pub fn domain_target(&self) -> String {
        domain_target(&self.identity.label())
    }
}
