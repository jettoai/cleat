//! One binary, two jobs (Swift `main.swift`). A bare launch, or one with flag-shaped arguments only
//! (LaunchServices passes `-psn_...`), is the daemon, unless a person opened the app while the
//! launchd daemon exists: that opens the settings window (`opened_by_hand`). A first argument that
//! is a word is a CLI subcommand. `run --observe [--trace]` is the observe-only daemon. The enforcing
//! daemon shows a menu bar item.

use std::process::ExitCode;
use std::sync::mpsc;

use cleat_rs::app::microphone;
use cleat_rs::audio::CoreAudioSystem;
use cleat_rs::cli;
use cleat_rs::config::paths;
use cleat_rs::engine::{self, Engine, EngineDeps, Mode};
use cleat_rs::identity::{self, Identity};
use cleat_rs::launch::{LaunchctlJob, Launchd, SmApp};
use cleat_rs::liveness::live_detectors;
use cleat_rs::model::MicrophonePermission;
use cleat_rs::outvol::writer_log::LogStream;
use cleat_rs::reclaim::{SmartRoutingClient, SystemProfilerPairings};
use cleat_rs::report::{self, Curl, Meta, Reporter, ReporterConfig};
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
        _ if !trace && opened_by_hand() => ExitCode::from(cleat_rs::settings::run(&[]).clamp(0, 255) as u8),
        _ => daemon(Mode::Enforce, trace),
    }
}

/// A bare launch from Finder, Raycast or the Dock (LaunchServices, not our launchd job) while the
/// launchd job exists opens the settings window: the daemon is launchd's, and a second one would
/// fight it over the same devices. A job that is loaded but down is kicked first, as the daemon's
/// own hand-over used to. With no job there is no other daemon, so this launch is the daemon.
fn opened_by_hand() -> bool {
    let job = LaunchctlJob { label: Identity::current().label() };
    if job.started_by_launchd() {
        return false;
    }
    let (loaded, pid) = job.loaded_job();
    if loaded && pid.is_none() {
        let _ = job.kickstart();
    }
    loaded
}

fn daemon(mode: Mode, trace: bool) -> ExitCode {
    let (tx, rx) = mpsc::channel();
    if mode == Mode::Observe {
        println!("cleat-rs observing (pid {}, log {})", std::process::id(), paths::log_path().display());
    }
    // Observe mode asks for nothing and reports nothing (D6).
    let observe = mode == Mode::Observe;
    let reporter = (!observe).then(make_reporter);
    if let Some(r) = &reporter {
        r.install_panic_hook();
    }
    let microphone_tx = tx.clone();
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
            error_reports_changed: match reporter {
                Some(r) => Box::new(move |on| r.set_enabled(on)),
                None => Box::new(|_| {}),
            },
            detectors: live_detectors(tx.clone()),
            routing: Box::new(routing_client(tx.clone())),
            inventory: Box::<SystemProfilerPairings>::default(),
            writer_source: {
                let tx = tx.clone();
                Box::new(move || Box::new(LogStream::new(tx.clone())))
            },
            events: Some(tx),
        };
        let mut eng = Engine::new(deps, mode, trace);
        // Engine first, dialog second: four of the five rules need no microphone.
        eng.start(if observe { MicrophonePermission::Pending } else { microphone::current() });
        engine::run(&mut eng, &rx);
    });
    if let Err(e) = spawned {
        eprintln!("cleat: could not start the engine thread: {e}");
        return ExitCode::FAILURE;
    }
    if !observe {
        microphone::request(microphone_tx);
    }
    if observe {
        // The HAL delivers listener callbacks through the main run loop.
        CFRunLoop::run();
    } else {
        // NSApplication runs the same main run loop, plus the menu bar item.
        let mtm = objc2::MainThreadMarker::new().expect("the daemon runs on the main thread");
        cleat_rs::app::menubar::run(mtm);
    }
    ExitCode::SUCCESS
}

/// The routing SPI, answering onto the engine's channel.
fn routing_client(tx: mpsc::Sender<engine::Event>) -> SmartRoutingClient {
    let bundle_id = Identity::current().bundle_id.unwrap_or_else(|| "ai.jetto.cleat".into());
    let tx = std::sync::Mutex::new(tx);
    SmartRoutingClient::new(
        bundle_id,
        std::sync::Arc::new(move |name, address, response| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(engine::Event::RouteAnswered { name, address, response });
            }
        }),
    )
}

fn make_reporter() -> Reporter {
    let identity = Identity::current();
    let version = identity.version();
    let build = identity::build_number().unwrap_or_else(|| "0".into());
    let home = paths::home();
    let cfg = ReporterConfig {
        support_dir: paths::support_dir(),
        diagnostics_dir: home.join("Library/Logs/DiagnosticReports"),
        home: home.display().to_string(),
        meta: Meta {
            release: format!("{}@{version}+{build}", identity.label()),
            environment: if identity.is_development_build() { "development" } else { "production" }.into(),
            os_version: report::os_version(),
        },
        probe: std::env::var("CLEAT_SENTRY_TEST_EVENT").is_ok_and(|v| v == "1"),
    };
    Reporter::new(cfg, Box::new(Curl { client: format!("cleat-rs/{version}") }))
}
