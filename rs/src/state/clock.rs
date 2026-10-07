//! Timestamps without a date crate, and the clock the engine schedules on.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What the engine reads time from. `mono` drives the scheduler and the balance settle window;
/// tests replace it so a 400ms wait costs nothing.
pub trait Clock {
    /// Monotonic time since an arbitrary origin.
    fn mono(&self) -> Duration;
    fn wall(&self) -> SystemTime;
}

pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn mono(&self) -> Duration {
        self.0.elapsed()
    }
    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}

fn split(t: SystemTime) -> (libc::tm, u32) {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as libc::time_t;
    // SAFETY: zeroed tm is a valid out-buffer for localtime_r.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call.
    unsafe { libc::localtime_r(&secs, &mut tm) };
    (tm, d.subsec_millis())
}

/// `yyyy-MM-dd HH:mm:ss`, local time: the Swift event log and `cleat status` format.
pub fn local_seconds(t: SystemTime) -> String {
    let (tm, _) = split(t);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// `HH:mm:ss.SSS`, local time.
pub fn clock_ms(t: SystemTime) -> String {
    let (tm, ms) = split(t);
    format!("{:02}:{:02}:{:02}.{:03}", tm.tm_hour, tm.tm_min, tm.tm_sec, ms)
}

/// `yyyy-MM-ddTHH:mm:ssZ`, UTC.
pub fn iso8601_utc(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as libc::time_t;
    // SAFETY: zeroed tm is a valid out-buffer for gmtime_r.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call.
    unsafe { libc::gmtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// Reads what `iso8601_utc` and Swift's `JSONEncoder` `.iso8601` write. Nothing else.
pub fn parse_iso8601_utc(s: &str) -> Option<SystemTime> {
    let b = s.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z'
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i32>().ok();
    // SAFETY: zeroed tm is valid input once the fields below are set.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = n(0..4)? - 1900;
    tm.tm_mon = n(5..7)? - 1;
    tm.tm_mday = n(8..10)?;
    tm.tm_hour = n(11..13)?;
    tm.tm_min = n(14..16)?;
    tm.tm_sec = n(17..19)?;
    // SAFETY: tm is a valid, initialised struct.
    let secs = unsafe { libc::timegm(&mut tm) };
    (secs >= 0).then(|| UNIX_EPOCH + Duration::from_secs(secs as u64))
}
