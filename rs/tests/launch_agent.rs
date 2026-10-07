//! The launch agent's pure parts (Swift LaunchAgentTests and the `LaunchAgent` helpers).

use cleat_rs::launch::{first_pid, is_registered, needs_reregistration, AgentStatus};

const ALL: [AgentStatus; 4] =
    [AgentStatus::Enabled, AgentStatus::NotRegistered, AgentStatus::NotFound, AgentStatus::RequiresApproval];

#[test]
fn only_an_enabled_registration_with_no_job_needs_registering_again() {
    for status in ALL {
        for loaded in [true, false] {
            let only_true_cell = status == AgentStatus::Enabled && !loaded;
            assert_eq!(needs_reregistration(status, || loaded), only_true_cell, "{status:?} loaded {loaded}");
        }
    }
}

#[test]
fn job_loaded_is_asked_only_for_an_enabled_registration() {
    for status in ALL {
        let mut asked = false;
        needs_reregistration(status, || {
            asked = true;
            true
        });
        assert_eq!(asked, status == AgentStatus::Enabled, "{status:?}");
    }
}

#[test]
fn requires_approval_counts_as_registered() {
    assert!(is_registered(AgentStatus::Enabled));
    assert!(is_registered(AgentStatus::RequiresApproval));
    assert!(!is_registered(AgentStatus::NotRegistered));
    assert!(!is_registered(AgentStatus::NotFound));
    assert!(!is_registered(AgentStatus::Unknown));
}

#[test]
fn first_pid_is_the_jobs_own() {
    let print = "gui/501/ai.jetto.cleat = {\n\tactive count = 1\n\tpid = 50568\n\tendpoints = {\n\t\tpid = 7\n\t}\n}";
    assert_eq!(first_pid(print), Some(50568));
    assert_eq!(first_pid("gui/501/ai.jetto.cleat = {\n\tstate = not running\n}"), None);
    assert_eq!(first_pid("pid = x\npid = 9"), None);
}
