//! Whether an output change away from a listed headset was the user's (B-1173, 2026-10-07
//! 20:52:40): the keyboard in the last 5 s and the Mac still holding the headset, both.

mod common;

use cleat_rs::model::DeviceSnapshot;
use common::reclaim::*;
use common::*;

/// Playing on the AirPods, with the wired headphones as the other output in CoreAudio.
fn on_the_headset(snap: &mut DeviceSnapshot) {
    snap.devices = vec![air_pods(), wired_headphones()];
    snap.default_output = Some(air_pods().id);
}

fn move_output(w: &common::engine::Harness, to: u32) {
    w.set(|s| s.default_output = Some(to));
}

fn mac_holds(w: &common::engine::Harness, owns: bool) {
    w.audio.owns.borrow_mut().insert(air_pods().id, owns);
}

/// 2026-10-07 20:50:48.874 the iPhone took the AirPods Max (ownership 0, ca-full 2256); the
/// output stayed on them until 20:52:40.019 the user pressed play on the Mac, and 280 ms later
/// coreaudiod moved 'dOut' to the wired headphones (ca-full 2690). Swift logged "the user picked
/// 外接耳機" and never asked again.
#[test]
fn macos_falling_back_after_the_phone_took_the_headset_is_not_the_users_choice() {
    let mut w = world_where(Some(media_hold()), on_the_headset);
    mac_holds(&w, true);
    w.reconcile();
    mac_holds(&w, false); // 20:50:48.874
    w.advance(secs(111.0));
    w.user(Some(0.28)); // 20:52:40.019 play
    move_output(&w, wired_headphones().id); // 20:52:40.300
    w.reconcile();
    assert_eq!(w.count("the user picked"), 0);
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    // Held by the phone: backed off, then asked again while the playback lasts.
    w.advance(secs(61.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
}

#[test]
fn hand_picked_output_while_the_mac_holds_the_headset_still_stands() {
    let mut w = world_where(None, on_the_headset);
    mac_holds(&w, true);
    w.user(Some(1.0));
    move_output(&w, wired_headphones().id);
    for _ in 0..10 {
        w.advance(secs(61.0));
        w.reconcile();
    }
    assert!(w.requests().is_empty());
    assert_eq!(w.count("not asked (the user picked 外接耳機)"), 1);
}

/// The Mac still holds the headset but nobody touched it in the last 5 s: macOS moved the output,
/// not the user.
#[test]
fn output_moved_while_the_mac_holds_the_headset_and_nobody_typed_is_not_the_users_choice() {
    let mut w = world_where(None, on_the_headset);
    mac_holds(&w, true);
    w.user(Some(6.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    assert_eq!(w.count("the user picked"), 0);
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

#[test]
fn unreadable_ownership_keeps_the_keyboard_rule_and_says_so_once() {
    let mut w = world_where(None, on_the_headset); // owns map empty: unreadable
    w.user(Some(1.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    move_output(&w, air_pods().id);
    w.reconcile();
    move_output(&w, wired_headphones().id);
    w.reconcile();
    assert!(w.requests().is_empty());
    assert_eq!(w.count("headset ownership unreadable"), 1);
}
