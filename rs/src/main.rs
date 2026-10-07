//! cleat-rs CLI. Stage 1 runs the engine only in observe mode.

use std::process::ExitCode;
use std::sync::mpsc;

use cleat_rs::audio::CoreAudioSystem;
use cleat_rs::config::paths;
use cleat_rs::engine::{self, Engine, Mode, Status};
use cleat_rs::state::EventLog;
use objc2_core_foundation::CFRunLoop;

const USAGE: &str = "usage: cleat-rs run --observe | status | log [-n N] | version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("status") => status(),
        Some("log") => log(&args[1..]),
        Some("version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> ExitCode {
    if args != ["--observe"] {
        eprintln!("cleat-rs: stage 1 runs only with --observe (writes to CoreAudio are not enabled in this build)");
        return ExitCode::from(2);
    }
    let (tx, rx) = mpsc::channel();
    println!("cleat-rs observing (pid {}, log {})", std::process::id(), paths::log_path().display());
    let spawned = std::thread::Builder::new().name("cleat-engine".into()).spawn(move || {
        let system = CoreAudioSystem::new(&tx);
        let log = EventLog::new(paths::log_path(), paths::rotated_log_path());
        let mut eng = Engine::new(Box::new(system), Mode::Observe, log, paths::config_path(), paths::status_path());
        eng.start(Some(tx));
        engine::run(&mut eng, &rx);
    });
    if let Err(e) = spawned {
        eprintln!("cleat-rs: could not start the engine thread: {e}");
        return ExitCode::FAILURE;
    }
    // The HAL delivers listener callbacks through the main run loop.
    CFRunLoop::run();
    ExitCode::SUCCESS
}

fn is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only checks existence.
    unsafe { libc::kill(pid, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM) }
}

fn pad(v: &str) -> String {
    let width = 12.max(v.chars().count() + 1);
    format!("{v:<width$}")
}

fn status() -> ExitCode {
    let path = paths::status_path();
    let Some(s) = Status::read(&path) else {
        println!("daemon:      not running (no status file at {})", paths::tilde(&path));
        return ExitCode::from(1);
    };
    let running = is_alive(s.pid);
    if running {
        println!("daemon:      running (pid {}, {})", s.pid, s.mode);
    } else {
        println!("daemon:      not running (last pid {})", s.pid);
    }
    println!("updated:     {}", s.updated_at);
    println!("config:      {} ({})", s.config_state, paths::tilde(&paths::config_path()));
    println!("microphone:  {}", s.microphone);
    println!("input:       {}", s.default_input.as_deref().unwrap_or("-"));
    println!("output:      {}", s.default_output.as_deref().unwrap_or("-"));
    if !s.rules.is_empty() {
        println!("rules:");
        for (k, v) in &s.rules {
            println!("  {} {v}", pad(k));
        }
    }
    if !s.liveness.is_empty() {
        println!("liveness:");
        for (k, v) in &s.liveness {
            println!("  {} {v}", pad(k));
        }
    }
    if !s.recent_events.is_empty() {
        println!("recent:");
        for e in &s.recent_events[s.recent_events.len().saturating_sub(5)..] {
            println!("  {e}");
        }
    }
    if running { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

fn log(args: &[String]) -> ExitCode {
    let n = match args {
        [] => 20,
        [flag, v] if flag == "-n" => match v.parse::<usize>() {
            Ok(n) if n > 0 => n,
            _ => {
                eprintln!("cleat-rs: -n wants a positive integer, got {v}");
                return ExitCode::from(2);
            }
        },
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let path = paths::log_path();
    if !path.exists() {
        println!("no events yet ({})", paths::tilde(&path));
        return ExitCode::SUCCESS;
    }
    for line in EventLog::tail(n, &path) {
        println!("{line}");
    }
    ExitCode::SUCCESS
}
