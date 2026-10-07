//! Timestamp formatting without a date crate.

use std::time::{SystemTime, UNIX_EPOCH};

fn split(t: SystemTime) -> (libc::tm, u32) {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as libc::time_t;
    // SAFETY: zeroed tm is a valid out-buffer for localtime_r.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the call.
    unsafe { libc::localtime_r(&secs, &mut tm) };
    (tm, d.subsec_millis())
}

/// `yyyy-MM-dd HH:mm:ss.SSS`, local time.
pub fn local_ms(t: SystemTime) -> String {
    let (tm, ms) = split(t);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        ms
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
