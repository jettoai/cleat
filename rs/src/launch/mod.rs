//! The launchd agent shipped inside the bundle (Swift `LaunchAgent.swift`), named and talked to
//! from one place so the daemon and the CLI cannot disagree about it.

mod service;

pub use service::{AgentService, AgentStatus, SmApp};

use std::io::Read;
use std::process::{Command, Stdio};

/// `.requiresApproval` is registered: the registration exists and only a person turns it on.
pub fn is_registered(status: AgentStatus) -> bool {
    matches!(status, AgentStatus::Enabled | AgentStatus::RequiresApproval)
}

/// An `.enabled` registration whose launchd job is gone (seen on a Homebrew upgrade). `job_loaded`
/// is only asked for an `.enabled` registration.
pub fn needs_reregistration(status: AgentStatus, job_loaded: impl FnOnce() -> bool) -> bool {
    status == AgentStatus::Enabled && !job_loaded()
}

pub fn domain_target(label: &str) -> String {
    // SAFETY: getuid cannot fail.
    format!("gui/{}/{label}", unsafe { libc::getuid() })
}

/// `launchctl print` puts the job's own `pid = N` first; a loaded job that is not running has none.
pub fn first_pid(output: &str) -> Option<i32> {
    let line = output.split('\n').map(str::trim).find(|l| l.starts_with("pid = "))?;
    line["pid = ".len()..].trim().parse().ok()
}

/// Runs `/bin/launchctl`, stdout and stderr together, trimmed. Output is read before the wait so
/// a full pipe cannot hang it.
pub fn launchctl(args: &[&str]) -> (i32, String) {
    let spawned = std::io::pipe().and_then(|(reader, writer)| {
        let child = Command::new("/bin/launchctl")
            .args(args)
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .spawn()?;
        Ok((reader, child))
    });
    // The Command (and with it both write ends) is gone here, so the read ends at the child's EOF.
    let (mut reader, mut child) = match spawned {
        Ok(x) => x,
        Err(e) => return (-1, e.to_string()),
    };
    let mut out = Vec::new();
    let _ = reader.read_to_end(&mut out);
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
    (code, String::from_utf8_lossy(&out).trim().to_string())
}

pub fn failure_detail(result: &(i32, String)) -> String {
    if result.1.is_empty() {
        format!("launchctl exited {}", result.0)
    } else {
        result.1.clone()
    }
}

/// launchd as the engine needs it, so the hand-over can be tested without exiting the test.
pub trait Launchd {
    /// Whether the job is loaded, and its pid if it is running.
    fn loaded_job(&self) -> (bool, Option<i32>);
    /// `launchctl kickstart gui/<uid>/<label>`.
    fn kickstart(&self) -> (i32, String);
    /// launchd names the job's service after the label; LaunchServices makes a name up.
    fn started_by_launchd(&self) -> bool;
    fn exit(&self);
}

pub struct LaunchctlJob {
    pub label: String,
}

impl Launchd for LaunchctlJob {
    fn loaded_job(&self) -> (bool, Option<i32>) {
        let (code, out) = launchctl(&["print", &domain_target(&self.label)]);
        if code != 0 {
            return (false, None);
        }
        (true, first_pid(&out))
    }
    fn kickstart(&self) -> (i32, String) {
        launchctl(&["kickstart", &domain_target(&self.label)])
    }
    fn started_by_launchd(&self) -> bool {
        std::env::var("XPC_SERVICE_NAME").ok().as_deref() == Some(self.label.as_str())
    }
    fn exit(&self) {
        std::process::exit(0);
    }
}
