//! kqueue on the watched folder, in the manner of `config/watcher.rs`. Every change in the folder's
//! first level sends one `()`; debounce is the worker's job. The vnode stays registered across
//! writes and is rebuilt only when the folder itself is deleted or renamed, so nothing falls
//! between two registrations. A folder that cannot be opened is retried every `RETRY`, and the
//! worker is told when that starts and when it ends, so it looks again.

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

const RETRY: Duration = Duration::from_secs(2);
/// Identifies the user event that ends the watch.
const STOP: usize = 1;

pub struct Watcher {
    kq: i32,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn spawn(dir: PathBuf, tx: Sender<()>) -> std::io::Result<Self> {
        // SAFETY: plain syscall; a negative result is handled below.
        let kq = unsafe { libc::kqueue() };
        if kq < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let stop = event(STOP, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0);
        // SAFETY: one change in, nothing out, non-blocking (zero timeout).
        let added = unsafe { libc::kevent(kq, &stop, 1, std::ptr::null_mut(), 0, &ZERO) };
        if added < 0 {
            // SAFETY: kq was opened above.
            unsafe { libc::close(kq) };
            return Err(std::io::Error::last_os_error());
        }
        let spawned = std::thread::Builder::new().name("cleat-photos-watch".into()).spawn(move || run(kq, &dir, &tx));
        match spawned {
            Ok(thread) => Ok(Self { kq, thread: Some(thread) }),
            Err(e) => {
                // SAFETY: kq was opened above and no thread uses it.
                unsafe { libc::close(kq) };
                Err(e)
            }
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let trigger = event(STOP, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER);
        // SAFETY: kq stays open until the close below, whether or not the thread has ended.
        unsafe { libc::kevent(self.kq, &trigger, 1, std::ptr::null_mut(), 0, &ZERO) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        // SAFETY: kq was opened by `spawn`, the thread is joined, and nothing else closes it.
        unsafe { libc::close(self.kq) };
    }
}

const ZERO: libc::timespec = libc::timespec { tv_sec: 0, tv_nsec: 0 };

fn event(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent { ident, filter, flags, fflags, data: 0, udata: std::ptr::null_mut() }
}

fn run(kq: i32, dir: &std::path::Path, tx: &Sender<()>) {
    let Ok(c_dir) = CString::new(dir.as_os_str().as_encoded_bytes()) else { return };
    let mut unreadable = false;
    loop {
        // SAFETY: c_dir is a valid NUL-terminated string.
        let fd = unsafe { libc::open(c_dir.as_ptr(), libc::O_EVTONLY) };
        // Told once when the trouble starts and once when it ends, not every retry.
        if (fd < 0) != unreadable {
            unreadable = fd < 0;
            if tx.send(()).is_err() {
                if fd >= 0 {
                    // SAFETY: fd was opened above.
                    unsafe { libc::close(fd) };
                }
                break;
            }
        }
        let vnode_flags = libc::NOTE_WRITE | libc::NOTE_RENAME | libc::NOTE_DELETE | libc::NOTE_ATTRIB | libc::NOTE_EXTEND;
        let change = event(fd as usize, libc::EVFILT_VNODE, libc::EV_ADD | libc::EV_CLEAR, vnode_flags);
        let retry = libc::timespec { tv_sec: RETRY.as_secs() as _, tv_nsec: 0 };
        let mut changes = i32::from(fd >= 0);
        let mut ended = false;
        loop {
            // SAFETY: zeroed kevent is a valid out-buffer.
            let mut out: libc::kevent = unsafe { std::mem::zeroed() };
            let timeout = if fd < 0 { &retry as *const _ } else { std::ptr::null() };
            // SAFETY: up to one change in, one event out; blocks until an event, or `retry` when
            // the folder could not be opened (null timeout otherwise).
            let n = unsafe { libc::kevent(kq, &change, changes, &mut out, 1, timeout) };
            changes = 0;
            if n < 0 || n > 0 && out.filter == libc::EVFILT_USER {
                ended = true;
                break;
            }
            if n == 0 {
                break; // the retry elapsed: open again
            }
            if tx.send(()).is_err() {
                ended = true;
                break;
            }
            if out.fflags & (libc::NOTE_DELETE | libc::NOTE_RENAME) != 0 {
                break; // the folder itself went: register the new one
            }
        }
        if fd >= 0 {
            // SAFETY: fd was opened above; closing it drops its knote.
            unsafe { libc::close(fd) };
        }
        if ended {
            break;
        }
    }
    // kq is not closed here: `Drop` owns it, so its number is never free while a `Watcher` can use it.
}
