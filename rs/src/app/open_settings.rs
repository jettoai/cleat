//! Brings the settings window up from the daemon's menu. Through LaunchServices, so the new process
//! is activated like any app a person opened and is not in the launchd job's process group (a
//! `cleat restart` must not take the window down with the daemon). A bare binary (no bundle) falls
//! back to spawning itself in a new session.

use block2::RcBlock;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSBundle, NSError, NSString};

pub fn open_settings() {
    if !open_through_launch_services() {
        spawn_detached();
    }
}

/// False when there is no .app to open (a bare `cargo run`). A failure LaunchServices reports
/// later is logged once and not retried.
fn open_through_launch_services() -> bool {
    let bundle = NSBundle::mainBundle();
    if !bundle.bundlePath().to_string().ends_with(".app") {
        return false;
    }
    let cfg = NSWorkspaceOpenConfiguration::configuration();
    cfg.setCreatesNewApplicationInstance(true);
    cfg.setArguments(&NSArray::from_retained_slice(&[NSString::from_str("settings")]));
    cfg.setActivates(true);
    let done = RcBlock::new(|_app: *mut NSRunningApplication, error: *mut NSError| {
        // SAFETY: LaunchServices passes a valid NSError or null.
        if let Some(e) = unsafe { error.as_ref() } {
            eprintln!("cleat: could not open the settings window: {}", e.localizedDescription());
        }
    });
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &bundle.bundleURL(),
        &cfg,
        Some(&done),
    );
    true
}

fn spawn_detached() {
    use std::os::unix::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = std::process::Command::new(exe);
    // Inherited, XPC_SERVICE_NAME would make the child think launchd started it.
    cmd.arg("settings").env_remove("XPC_SERVICE_NAME");
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        })
    };
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
