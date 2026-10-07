//! The daemon's CPU and memory, read from outside like Activity Monitor (Swift `DaemonVitals.swift`
//! and `Core/State/ProcessVitals.swift`). The sampler is pure; `read_process` is the one syscall.

use crate::state::reaction_clock::DaemonPerformance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VitalsState {
    Running,
    NotRunning,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DaemonVitals {
    pub state: VitalsState,
    pub cpu_percent: Option<f64>,
    pub cpu_measuring: bool,
    pub cpu_window_seconds: Option<i64>,
    pub footprint_bytes: Option<u64>,
    /// status.json `performance`; None for a daemon older than the measurement.
    pub performance: Option<DaemonPerformance>,
}

impl DaemonVitals {
    pub fn with_state(state: VitalsState) -> Self {
        Self {
            state,
            cpu_percent: None,
            cpu_measuring: false,
            cpu_window_seconds: None,
            footprint_bytes: None,
            performance: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub pid: i32,
    pub cpu_nanos: u64,
    pub footprint_bytes: u64,
    pub wall_nanos: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NotRunning,
    Unreadable(i32),
}

pub const CPU_WINDOW_NS: u64 = 60_000_000_000;

/// One core fully busy is 100%. None across a restart, a counter gone backwards, or no time.
pub fn cpu_percent(from: &Reading, to: &Reading) -> Option<f64> {
    if from.pid != to.pid || to.wall_nanos <= from.wall_nanos || to.cpu_nanos < from.cpu_nanos {
        return None;
    }
    Some((to.cpu_nanos - from.cpu_nanos) as f64 / (to.wall_nanos - from.wall_nanos) as f64 * 100.0)
}

pub fn mach_ticks_to_nanos(ticks: u64, numer: u32, denom: u32) -> u64 {
    if denom == 0 {
        return ticks;
    }
    (ticks as u128 * numer as u128 / denom as u128) as u64
}

/// CPU averaged over the last minute of readings of one pid.
#[derive(Debug, Default)]
pub struct Sampler {
    history: Vec<Reading>,
}

impl Sampler {
    pub fn sample(&mut self, pid: Option<i32>, read: impl Fn(i32) -> Result<Reading, Failure>) -> DaemonVitals {
        let Some(pid) = pid else {
            self.history.clear();
            return DaemonVitals::with_state(VitalsState::NotRunning);
        };
        match read(pid) {
            Err(Failure::NotRunning) => {
                self.history.clear();
                DaemonVitals::with_state(VitalsState::NotRunning)
            }
            Err(Failure::Unreadable(_)) => {
                self.history.clear();
                DaemonVitals::with_state(VitalsState::Unreadable)
            }
            Ok(reading) => {
                let mut v = DaemonVitals::with_state(VitalsState::Running);
                v.footprint_bytes = Some(reading.footprint_bytes);
                let Some(last) = self.history.last().copied().filter(|l| l.pid == reading.pid) else {
                    self.history = vec![reading];
                    v.cpu_measuring = true;
                    return v;
                };
                if reading.cpu_nanos < last.cpu_nanos || reading.wall_nanos <= last.wall_nanos {
                    self.history = vec![reading];
                    return v;
                }
                self.history.push(reading);
                while self.history.len() > 2 && self.history[1].wall_nanos + CPU_WINDOW_NS <= reading.wall_nanos {
                    self.history.remove(0);
                }
                let base = self.history[0];
                v.cpu_percent = cpu_percent(&base, &reading);
                v.cpu_window_seconds = Some(((reading.wall_nanos - base.wall_nanos) as f64 / 1e9).round() as i64);
                v
            }
        }
    }
}

/// One `proc_pid_rusage` reading. A pid reused by something not named Cleat is as good as gone.
#[allow(deprecated)] // mach_timebase_info: libc points at the mach2 crate, not added for one call.
pub fn read_process(pid: i32) -> Result<Reading, Failure> {
    if pid <= 0 {
        return Err(Failure::NotRunning);
    }
    let mut name = [0u8; 256];
    // SAFETY: `name` is a 256-byte buffer.
    let n = unsafe { libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
    if n > 0 && !name[..n as usize].starts_with(b"Cleat") {
        return Err(Failure::NotRunning);
    }
    // SAFETY: all-zero is a valid rusage_info_v4.
    let mut info: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is the struct the V4 flavour fills.
    let rc = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, (&mut info as *mut libc::rusage_info_v4).cast()) };
    if rc != 0 {
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        return Err(if errno == libc::ESRCH { Failure::NotRunning } else { Failure::Unreadable(errno) });
    }
    let mut tb = libc::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: `tb` is a valid out-pointer.
    unsafe { libc::mach_timebase_info(&mut tb) };
    let ticks = info.ri_user_time.wrapping_add(info.ri_system_time);
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid out-pointer.
    unsafe { libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut ts) };
    let wall = ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64;
    Ok(Reading {
        pid,
        cpu_nanos: mach_ticks_to_nanos(ticks, tb.numer, tb.denom),
        footprint_bytes: info.ri_phys_footprint,
        wall_nanos: wall,
    })
}
