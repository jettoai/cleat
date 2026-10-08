//! `cleat status` and `cleat log` as pure renderers, so their output can be compared byte for
//! byte with the Swift build's.

use std::path::Path;

use crate::config::paths;
use crate::engine::Status;
use crate::identity::Identity;
use crate::launch::{AgentService, AgentStatus, LaunchctlJob, Launchd, SmApp};
use crate::state::clock::{local_seconds, parse_iso8601_utc};
use crate::settings::text::millis_text;
use crate::state::reaction_clock::DaemonPerformance;
use crate::state::EventLog;

fn pad(v: &str) -> String {
    let width = 12.max(v.chars().count() + 1);
    format!("{v:<width$}")
}

/// `kill(pid, 0)`; EPERM means it exists and belongs to somebody else.
fn is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only checks existence.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

/// What `status` says about the launchd agent (Swift `CLI.supervision`).
pub fn supervision(agent: &dyn AgentService, job_loaded: impl FnOnce() -> bool) -> String {
    match agent.status() {
        AgentStatus::Enabled => {
            if job_loaded() {
                "launchd agent (registered)".into()
            } else {
                "launchd agent (registered but not loaded - run cleat restart)".into()
            }
        }
        AgentStatus::NotRegistered | AgentStatus::NotFound => "launchd agent (not registered)".into(),
        AgentStatus::RequiresApproval => {
            "launchd agent (waiting for approval in System Settings > General > Login Items)".into()
        }
        AgentStatus::Unknown => "launchd agent (unknown state)".into(),
    }
}

/// The whole `status` output and its exit code. `status_path` and `config_path` are already in
/// display (tilde) form.
pub fn render_status(
    status: Option<&Status>,
    alive: bool,
    supervision: &str,
    status_path: &str,
    config_path: &str,
) -> (String, i32) {
    let mut out = String::new();
    let mut line = |s: String| {
        out.push_str(&s);
        out.push('\n');
    };
    let Some(s) = status else {
        line(format!("daemon:      not running (no status file at {status_path})"));
        line(format!("supervision: {supervision}"));
        return (out, 1);
    };
    line(if alive {
        format!("daemon:      running (pid {})", s.pid)
    } else {
        format!("daemon:      not running (last seen as pid {})", s.pid)
    });
    line(format!("supervision: {supervision}"));
    let updated = parse_iso8601_utc(&s.updated_at).map_or_else(|| s.updated_at.clone(), local_seconds);
    line(format!("updated:     {updated}"));
    line(format!("config:      {} ({config_path})", s.config_state));
    line(format!("microphone:  {}", s.microphone));
    let reports = match s.error_reports {
        Some(true) => "error reports on",
        Some(false) => "error reports off",
        None => "-",
    };
    line(format!("reports:     {reports}"));
    line(format!("input:       {}", s.default_input.as_deref().unwrap_or("-")));
    line(format!("output:      {}", s.default_output.as_deref().unwrap_or("-")));
    if alive {
        line(reaction_line(s.performance.as_ref()));
    }
    for (title, section) in [("rules:", &s.rules), ("liveness:", &s.liveness)] {
        if !section.is_empty() {
            line(title.into());
            for (k, v) in section {
                line(format!("  {} {v}", pad(k)));
            }
        }
    }
    if !s.recent_events.is_empty() {
        line("recent:".into());
        for e in &s.recent_events[s.recent_events.len().saturating_sub(5)..] {
            line(format!("  {e}"));
        }
    }
    (out, if alive { 0 } else { 1 })
}

/// Swift `CLI.printVitals`'s reaction line; the `vitals:` line before it is not ported.
fn reaction_line(p: Option<&DaemonPerformance>) -> String {
    let Some(p) = p else { return "reaction:    -".into() };
    match (p.samples, p.last_reaction_ms, p.last_work_ms, p.median_reaction_ms) {
        (1.., Some(last), Some(work), Some(median)) => format!(
            "reaction:    last {} (work {}), median {} over {}",
            millis_text(last),
            millis_text(work),
            millis_text(median),
            p.samples
        ),
        _ => "reaction:    no write-back yet".into(),
    }
}

/// `log -n count`: the last lines, or the no-events line when there are none.
pub fn render_log(count: usize, path: &Path) -> String {
    let lines = EventLog::tail(count, path);
    if lines.is_empty() {
        return format!("no events yet ({})\n", paths::tilde(path));
    }
    lines.iter().map(|l| format!("{l}\n")).collect()
}

pub(super) fn status() -> i32 {
    let label = Identity::current().label();
    let agent = SmApp::agent(&format!("{label}.plist"));
    let job = LaunchctlJob { label };
    let sup = supervision(&agent, || job.loaded_job().0);
    let path = paths::status_path();
    let status = Status::read(&path);
    let alive = status.as_ref().is_some_and(|s| is_alive(s.pid));
    let (text, code) =
        render_status(status.as_ref(), alive, &sup, &paths::tilde(&path), &paths::tilde(&paths::config_path()));
    print!("{text}");
    code
}
