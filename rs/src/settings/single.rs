//! One settings window: the process holds an flock on <support>/settings.lock with its pid in it.
//! A second `cleat settings` finds the lock held, brings the holder forward and exits.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

#[derive(Debug)]
pub enum Lock {
    /// Keep it open for the life of the process; the lock goes with the file.
    Held(File),
    TakenBy(Option<i32>),
}

pub fn acquire(path: &Path) -> std::io::Result<Lock> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    // SAFETY: a valid, open fd.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        return Ok(Lock::TakenBy(s.trim().parse().ok()));
    }
    f.set_len(0)?;
    f.rewind()?;
    write!(f, "{}", std::process::id())?;
    Ok(Lock::Held(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cleat-single-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("settings.lock")
    }

    #[test]
    fn second_acquire_sees_first_pid() {
        let path = temp_lock("second");
        let first = acquire(&path).unwrap();
        assert!(matches!(first, Lock::Held(_)));
        // flock belongs to the open file description, so a second open in this process is refused.
        match acquire(&path).unwrap() {
            Lock::TakenBy(pid) => assert_eq!(pid, Some(std::process::id() as i32)),
            Lock::Held(_) => panic!("second acquire got the lock"),
        }
        drop(first);
    }

    #[test]
    fn lock_released_on_drop() {
        let path = temp_lock("drop");
        drop(acquire(&path).unwrap());
        assert!(matches!(acquire(&path).unwrap(), Lock::Held(_)));
    }
}
