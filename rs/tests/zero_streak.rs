//! ZeroStreakTests, test for test.

use cleat_rs::liveness::{Flip, ZeroStreak};

#[test]
fn nine_zero_frames_below_threshold_do_not_flip() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(9, true), None);
    assert!(!s.is_silent());
}

#[test]
fn threshold_reached_flips_to_silent() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(9, true), None);
    assert_eq!(s.feed(1, true), Some(Flip::BecameSilent));
    assert!(s.is_silent());
}

#[test]
fn exactly_threshold_in_one_buffer_flips() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(10, true), Some(Flip::BecameSilent));
}

#[test]
fn staying_silent_does_not_flip_again() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(10, true), Some(Flip::BecameSilent));
    assert_eq!(s.feed(10, true), None);
    assert_eq!(s.feed(1000, true), None);
}

#[test]
fn one_non_zero_buffer_flips_back_immediately() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(10, true), Some(Flip::BecameSilent));
    assert_eq!(s.feed(1, false), Some(Flip::BecameLive));
    assert!(!s.is_silent());
}

#[test]
fn first_buffer_reports_live() {
    // Otherwise a device that is fine from the start reads as "measuring" forever.
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(512, false), Some(Flip::BecameLive));
}

#[test]
fn live_state_does_not_flip_on_more_signal() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(512, false), Some(Flip::BecameLive));
    assert_eq!(s.feed(512, false), None);
}

#[test]
fn partial_silence_resets_the_count() {
    let mut s = ZeroStreak::new(10);
    assert_eq!(s.feed(9, true), None);
    assert_eq!(s.feed(1, false), Some(Flip::BecameLive));
    assert_eq!(s.zero_frames(), 0);
    // The count restarts, so nine more zero frames are still not enough.
    assert_eq!(s.feed(9, true), None);
}

#[test]
fn threshold_is_at_least_one_frame() {
    let mut s = ZeroStreak::new(0);
    assert_eq!(s.threshold_frames(), 1);
    assert_eq!(s.feed(1, true), Some(Flip::BecameSilent));
}
