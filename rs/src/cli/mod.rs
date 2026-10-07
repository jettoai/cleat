//! `cleat status`, `cleat log`, `cleat restart`, `cleat version`: Swift `CLI.swift`, output and
//! exit codes word for word. The CLI never talks to the daemon; it reads the files it writes.

mod restart;
mod status;

pub use status::{render_log, render_status, supervision};

use crate::config::paths;
use crate::identity::Identity;

const DEFAULT_LOG_LINES: usize = 50;

const USAGE: &str = "cleat - audio device keeper

usage:
  cleat status        what the daemon is holding right now
  cleat log [-n 50]   recent events
  cleat restart       start the daemon, or replace the running one, through the
                      launchd agent that keeps it alive
  cleat reclaim [device]
                      ask a Bluetooth headset back from whatever took it, once,
                      and print the answer. Defaults to the config's \"reclaim\" list
  cleat version

Running Cleat.app with no arguments starts the daemon. launchd starts it at login and
starts it again if it is killed; `cleat restart` is how it comes back after a clean quit,
which is what a Homebrew upgrade does.
";

/// Runs one subcommand and returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let Some(command) = args.first() else { return usage() };
    match command.as_str() {
        "status" => status::status(),
        "log" => log(&args[1..]),
        "restart" => restart::restart(),
        "version" | "--version" => {
            println!("{}", Identity::current().version());
            0
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            0
        }
        // The reclaim subcommand arrives with the reclaim port.
        _ => {
            eprintln!("cleat: unknown command '{command}'");
            usage()
        }
    }
}

fn usage() -> i32 {
    eprint!("{USAGE}");
    2
}

fn log(args: &[String]) -> i32 {
    let mut count = DEFAULT_LOG_LINES;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-n" || a == "--lines" {
            match args.get(i + 1).and_then(|v| v.parse::<i64>().ok()) {
                Some(v) if v > 0 => count = v as usize,
                _ => {
                    eprintln!("cleat log: -n needs a positive number");
                    return 2;
                }
            }
            i += 2;
        } else {
            eprintln!("cleat log: unexpected argument '{a}'");
            return 2;
        }
    }
    print!("{}", render_log(count, &paths::log_path()));
    0
}

/// Writes `cleat <message>` to stderr and returns 1.
fn fail(message: &str) -> i32 {
    eprintln!("cleat {message}");
    1
}
