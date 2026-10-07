//! A tool held at arm's length (Swift `ChildProcess`, `BluetoothHeadsets.swift:182-271`): its
//! output is read on a thread that may stay parked as long as the tool likes, while the caller
//! waits only as long as it agreed to. Signals cannot be relied on to end a read: a child that
//! ignores SIGTERM, or one that forks a grandchild holding the pipe and exits 0, both kept Swift's
//! reader parked past the deadline. The deadline is therefore on this side of the pipe.

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// `reaped` is set under the lock in the same breath as the child is waited for, so a signal sent
/// under the lock never reaches a pid the system has handed on.
struct Run {
    pid: libc::pid_t,
    reaped: bool,
}

/// What `program` printed, when it exited 0 inside `timeout`; `None` otherwise.
pub fn run_with_deadline(program: &str, args: &[&str], timeout: Duration) -> Option<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let run = Arc::new(Mutex::new(Run { pid: child.id() as libc::pid_t, reaped: false }));
    let (done_tx, done_rx) = mpsc::channel();
    let reader = run.clone();
    std::thread::spawn(move || {
        let mut data = vec![];
        let read_ok = stdout.read_to_end(&mut data).is_ok();
        // Wait for the exit without reaping, outside the lock, so `signal` is never stuck behind
        // a child that takes its time; then reap under the lock.
        wait_without_reaping(child.id() as libc::pid_t);
        let clean = match reader.lock() {
            Ok(mut r) => {
                r.reaped = true;
                child.wait().is_ok_and(|s| s.success())
            }
            Err(_) => false,
        };
        // Both halves have to hold: EOF alone is also what a tool that printed half a document
        // and exited 0 gives.
        let _ = done_tx.send((read_ok && clean).then_some(data));
    });
    match done_rx.recv_timeout(timeout) {
        Ok(output) => output,
        Err(_) => {
            abandon(&run);
            None
        }
    }
}

/// Nobody is waiting any more: ask the run to leave, insist a second later.
fn abandon(run: &Arc<Mutex<Run>>) {
    signal(run, libc::SIGTERM);
    let later = run.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(1));
        signal(&later, libc::SIGKILL);
    });
}

/// Signals the run, group and all, unless it has already been waited for. The group is only
/// signalled when the child leads it; otherwise it would be ours.
fn signal(run: &Arc<Mutex<Run>>, number: i32) {
    let Ok(r) = run.lock() else { return };
    if r.reaped {
        return;
    }
    let pid = r.pid;
    // SAFETY: plain syscalls on a pid we spawned and have not reaped (held under the lock).
    unsafe {
        let target = if libc::getpgid(pid) == pid { -pid } else { pid };
        libc::kill(target, number);
    }
}

/// Blocks until `pid` has exited, leaving it a zombie (so its pid is not reused yet).
fn wait_without_reaping(pid: libc::pid_t) {
    // SAFETY: zeroed siginfo is a valid out-buffer; WNOWAIT leaves the child for `Child::wait`.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: as above.
        let r = unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT) };
        if r == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return;
        }
    }
}
