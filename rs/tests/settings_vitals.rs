use std::cell::Cell;

use cleat_rs::settings::vitals::{Failure, Reading, Sampler, VitalsState};

fn reading(pid: i32, cpu_ms: u64, wall_s: u64) -> Reading {
    Reading { pid, cpu_nanos: cpu_ms * 1_000_000, footprint_bytes: 1 << 20, wall_nanos: wall_s * 1_000_000_000 }
}

#[test]
fn first_reading_measures_then_the_second_has_a_figure() {
    let mut s = Sampler::default();
    let v = s.sample(Some(7), |_| Ok(reading(7, 0, 100)));
    assert!(v.cpu_measuring && v.cpu_percent.is_none());
    let v = s.sample(Some(7), |_| Ok(reading(7, 10, 101)));
    assert_eq!(v.cpu_percent, Some(1.0));
    assert_eq!(v.cpu_window_seconds, Some(1));
}

#[test]
fn a_new_pid_or_a_counter_going_back_starts_over() {
    let mut s = Sampler::default();
    s.sample(Some(7), |_| Ok(reading(7, 100, 100)));
    assert!(s.sample(Some(8), |_| Ok(reading(8, 100, 101))).cpu_measuring);
    let v = s.sample(Some(8), |_| Ok(reading(8, 50, 102)));
    assert!(v.cpu_percent.is_none() && !v.cpu_measuring);
}

#[test]
fn the_window_keeps_one_minute() {
    let mut s = Sampler::default();
    let t = Cell::new(0u64);
    let mut last = None;
    for _ in 0..80 {
        t.set(t.get() + 1);
        last = Some(s.sample(Some(7), |_| Ok(reading(7, t.get() * 10, t.get()))));
    }
    let v = last.unwrap();
    assert_eq!(v.cpu_window_seconds, Some(60));
    assert_eq!(v.cpu_percent.map(|c| (c * 100.0).round()), Some(100.0));
}

#[test]
fn not_running_and_unreadable() {
    let mut s = Sampler::default();
    assert_eq!(s.sample(None, |_| Ok(reading(1, 0, 1))).state, VitalsState::NotRunning);
    assert_eq!(s.sample(Some(1), |_| Err(Failure::NotRunning)).state, VitalsState::NotRunning);
    assert_eq!(s.sample(Some(1), |_| Err(Failure::Unreadable(1))).state, VitalsState::Unreadable);
}

/// B-1287: the development build is `cleat-rs`; a reused pid named otherwise is not Cleat.
#[test]
fn any_cleat_process_name_counts() {
    use cleat_rs::settings::vitals::is_cleat_name;
    let got: Vec<bool> = ["cleat-rs", "Cleat", "Cleat Dev", "Finder"].iter().map(|n| is_cleat_name(n.as_bytes())).collect();
    assert_eq!(got, [true, true, true, false]);
    assert!(!is_cleat_name(b"Clea"));
}
