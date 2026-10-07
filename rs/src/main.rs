//! One binary, two jobs (Swift `main.swift`). A bare launch, or one with flag-shaped arguments only
//! (LaunchServices passes `-psn_...`), is the daemon; a first argument that is a word is a CLI
//! subcommand. `run --observe [--trace]` is the observe-only daemon, kept from stage 1.

use std::process::ExitCode;
use std::sync::mpsc;

use cleat_rs::audio::CoreAudioSystem;
use cleat_rs::cli;
use cleat_rs::config::paths;
use cleat_rs::engine::{self, Engine, EngineDeps, Mode};
use cleat_rs::identity::Identity;
use cleat_rs::launch::{LaunchctlJob, SmApp};
use cleat_rs::model::MicrophonePermission;
use cleat_rs::state::clock::SystemClock;
use cleat_rs::state::EventLog;
use objc2_core_foundation::CFRunLoop;

const CLI_FLAGS: [&str; 3] = ["--help", "-h", "--version"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let trace = args.iter().any(|a| a == "--trace");
    match args.first().map(String::as_str) {
        Some("run") if args.get(1).map(String::as_str) == Some("--observe") => daemon(Mode::Observe, true),
        Some(first) if !first.starts_with('-') || CLI_FLAGS.contains(&first) => {
            ExitCode::from(cli::run(&args).clamp(0, 255) as u8)
        }
        _ => daemon(Mode::Enforce, trace),
    }
}

fn daemon(mode: Mode, trace: bool) -> ExitCode {
    let (tx, rx) = mpsc::channel();
    if mode == Mode::Observe {
        println!("cleat-rs observing (pid {}, log {})", std::process::id(), paths::log_path().display());
    }
    let spawned = std::thread::Builder::new().name("cleat-engine".into()).spawn(move || {
        let identity = Identity::current();
        let label = identity.label();
        let deps = EngineDeps {
            system: Box::new(CoreAudioSystem::new(&tx)),
            log: EventLog::new(paths::log_path(), paths::rotated_log_path()),
            config_path: paths::config_path(),
            status_path: paths::status_path(),
            clock: Box::<SystemClock>::default(),
            agent: Box::new(SmApp::agent(&format!("{label}.plist"))),
            login_item: Box::new(SmApp::main_app()),
            launchd: Box::new(LaunchctlJob { label }),
            identity,
            error_reports_changed: Box::new(|_| {}),
            events: Some(tx),
        };
        let mut eng = Engine::new(deps, mode, trace);
        // The microphone is not asked for until the liveness port.
        eng.start(MicrophonePermission::Pending);
        engine::run(&mut eng, &rx);
    });
    if let Err(e) = spawned {
        eprintln!("cleat: could not start the engine thread: {e}");
        return ExitCode::FAILURE;
    }
    // The HAL delivers listener callbacks through the main run loop.
    CFRunLoop::run();
    ExitCode::SUCCESS
}
