//! Watches the config file with kqueue and sends `Event::ConfigTouched` for every vnode event.
//! The watch is rebuilt after each event (an atomic save replaces the inode), and the parent
//! directory is watched while the file does not exist. Debounce is the engine's job.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

use crate::engine::Event;

const REDISCOVER: Duration = Duration::from_secs(2);

pub fn spawn(path: PathBuf, tx: Sender<Event>) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("cleat-config-watch".into()).spawn(move || run(&path, &tx))
}

fn watch_target(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }
    path.parent().filter(|p| p.exists()).map(Path::to_path_buf)
}

fn run(path: &Path, tx: &Sender<Event>) {
    // SAFETY: plain syscall; a negative result is handled below.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return;
    }
    loop {
        let Some(target) = watch_target(path) else {
            std::thread::sleep(REDISCOVER);
            continue;
        };
        let Ok(c_path) = CString::new(target.as_os_str().as_encoded_bytes()) else { return };
        // SAFETY: c_path is a valid NUL-terminated string.
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_EVTONLY) };
        if fd < 0 {
            std::thread::sleep(REDISCOVER);
            continue;
        }
        let change = libc::kevent {
            ident: fd as usize,
            filter: libc::EVFILT_VNODE,
            flags: libc::EV_ADD | libc::EV_CLEAR,
            fflags: libc::NOTE_WRITE | libc::NOTE_RENAME | libc::NOTE_DELETE | libc::NOTE_ATTRIB | libc::NOTE_EXTEND,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: zeroed kevent is a valid out-buffer.
        let mut out: libc::kevent = unsafe { std::mem::zeroed() };
        // SAFETY: one change in, one event out, blocking (null timeout).
        let n = unsafe { libc::kevent(kq, &change, 1, &mut out, 1, std::ptr::null()) };
        // SAFETY: fd was opened above; closing it drops its knote.
        unsafe { libc::close(fd) };
        if n < 0 {
            std::thread::sleep(REDISCOVER);
            continue;
        }
        if tx.send(Event::ConfigTouched { received: SystemTime::now() }).is_err() {
            break;
        }
    }
    // SAFETY: kq was opened above.
    unsafe { libc::close(kq) };
}
