//! Rule 7's engine bookkeeping, ported from Swift `ReclaimTests.swift` (the 26 engine tests,
//! 0.3.11). `world.start()` is the harness constructor; `advance` walks the clock without firing.

mod common;

use common::engine::{Harness, Opts};
use common::reclaim::*;
use common::*;

const INTERVAL: f64 = 30.0;
const BACKOFF: f64 = 60.0;
const DELAY: f64 = 8.0;
const SPAN: f64 = 180.0;
const DELAY_MS: u64 = 8000;

fn here(snap: &mut cleat_rs::model::DeviceSnapshot) {
    snap.devices = vec![mac_studio_speakers(), air_pods()];
}

fn on_headset(snap: &mut cleat_rs::model::DeviceSnapshot) {
    snap.devices = vec![air_pods(), mac_studio_speakers()];
    snap.default_output = Some(air_pods().id);
}

#[test]
fn accepted_hijack_is_logged_once_and_then_throttled() {
    let mut w = world(Some(hijacked()));
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    assert_eq!(w.count("hijack accepted"), 1);
    w.reconcile();
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS, AIR_PODS_ADDRESS]));
}

#[test]
fn silent_mac_is_not_worth_reading_the_pairing_list_for() {
    let mut w = world_where(None, |s| s.output_running = false);
    w.reconcile();
    assert_eq!(w.pairings.reads.get(), 0);
    assert!(w.requests().is_empty());
    w.set(|s| s.output_running = true);
    w.reconcile();
    assert_eq!(w.pairings.reads.get(), 1);
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

#[test]
fn request_carries_the_hijack_score_and_reason() {
    let w = world(Some(hijacked()));
    assert_eq!(*w.routing.scores.borrow(), vec![201]);
    assert_eq!(
        *w.routing.reasons.borrow(),
        vec![cleat_rs::rules::reclaim::request_reason(&connected_air_pods())]
    );
}

#[test]
fn remote_hold_is_logged_once_per_spell_and_backs_off() {
    let mut w = world(Some(media_hold()));
    assert_eq!(w.requests().len(), 1);
    assert_eq!(w.count("held by remote device (Remote Category 301 > Local Category 200"), 1);
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(BACKOFF));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    assert_eq!(w.count("held by remote device"), 1);
    w.set_answer(Some(hijacked()));
    w.advance(secs(BACKOFF + 1.0));
    w.reconcile();
    w.set_answer(Some(on_a_call()));
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.count("held by remote device"), 2);
}

#[test]
fn already_routed_is_not_logged() {
    let w = world(Some(already_routed()));
    assert_eq!(w.requests().len(), 1);
    assert_eq!(w.count("reclaim:"), 0);
}

#[test]
fn busy_answer_clears_the_throttle_and_says_nothing() {
    let mut w = world(Some(answer(0, "Previous hijack hasn't finished")));
    assert_eq!(w.requests().len(), 1);
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    assert_eq!(w.count("reclaim:"), 0);
}

#[test]
fn unanswered_request_stops_blocking_after_the_interval() {
    let mut w = world(None);
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
}

#[test]
fn backed_off_headset_does_not_starve_the_other_one() {
    let mut w = world_with(
        reclaim_config(&["AirPods Max", "AirPods Pro"]),
        Some(media_hold()),
        vec![connected_air_pods_pro(), connected_air_pods()],
        playing(),
    );
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    w.reconcile();
    assert_eq!(w.requests(), vec![AIR_PODS_ADDRESS.to_string(), connected_air_pods_pro().address]);
}

#[test]
fn unavailable_routing_is_reported_once_and_never_asks() {
    let mut w = Harness::new(
        &reclaim_config(&["AirPods Max"]),
        playing(),
        Opts { routing_available: false, headsets: vec![connected_air_pods()], ..Opts::default() },
    );
    w.reconcile();
    w.reconcile();
    assert!(w.requests().is_empty());
    assert_eq!(w.count("reclaim: unavailable on this macOS"), 1);
    assert_eq!(w.rule("reclaim"), "unavailable (no routing service on this macOS) (AirPods Max)");
}

#[test]
fn rule_off_does_not_touch_bluetooth() {
    let w = world_with(reclaim_config(&[]), None, vec![connected_air_pods()], playing());
    assert_eq!(w.pairings.reads.get(), 0);
    assert!(w.requests().is_empty());
    assert_eq!(w.rule("reclaim"), "off");
}

#[test]
fn status_names_the_listed_headsets() {
    let w = world_with(
        reclaim_config(&["AirPods Max", "70:F9:4A:B6:0C:C9"]),
        Some(already_routed()),
        vec![connected_air_pods()],
        playing(),
    );
    assert_eq!(w.rule("reclaim"), "on (AirPods Max, 70:F9:4A:B6:0C:C9)");
}

#[test]
fn changed_reason_is_logged_again() {
    let mut w = world(Some(out_of_ear()));
    w.set_answer(Some(on_a_call()));
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    assert_eq!(w.count("refused (Buds out of ear)"), 1);
    assert_eq!(w.count("Remote Category 501"), 1);
    w.advance(secs(BACKOFF + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 3);
    assert_eq!(w.count("reclaim:"), 2);
}

#[test]
fn out_of_ear_retries_soon_for_a_while_then_backs_off() {
    let mut w = world(Some(out_of_ear()));
    assert!(w.engine.reconcile_pending(DELAY_MS));
    w.advance(secs(DELAY - 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    w.advance(secs(SPAN));
    w.engine.cancel_reconcile(DELAY_MS);
    w.reconcile();
    assert_eq!(w.requests().len(), 3);
    assert!(!w.engine.reconcile_pending(DELAY_MS));
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 3);
    w.advance(secs(BACKOFF));
    w.reconcile();
    assert_eq!(w.requests().len(), 4);
    assert_eq!(w.count("out of ear"), 1);
}

#[test]
fn stopping_playback_reopens_the_retry_window() {
    let mut w = world(Some(out_of_ear()));
    w.advance(secs(100.0));
    w.set(|s| s.output_running = false);
    w.reconcile();
    w.set(|s| s.output_running = true);
    w.advance(secs(100.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 3);
}

#[test]
fn headset_coming_back_on_its_own_ends_the_spell() {
    let mut w = world(Some(out_of_ear()));
    w.set(on_headset);
    w.reconcile();
    w.set(|s| {
        s.devices = vec![mac_studio_speakers()];
        s.default_output = Some(mac_studio_speakers().id);
    });
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    assert_eq!(w.count("refused (Buds out of ear)"), 2);
}

#[test]
fn headset_coming_back_on_its_own_reopens_the_retry_window() {
    let mut w = world(Some(out_of_ear()));
    w.advance(secs(10.0));
    w.set(on_headset);
    w.reconcile();
    w.advance(secs(SPAN + 20.0));
    w.set(|s| {
        s.devices = vec![mac_studio_speakers()];
        s.default_output = Some(mac_studio_speakers().id);
    });
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 3);
}

#[test]
fn call_hold_says_cleat_yields_by_design() {
    let mut w = world(Some(on_a_call()));
    assert_eq!(w.count("the phone is on a call, Cleat yields by design"), 1);
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
}

#[test]
fn headset_here_but_not_chosen_is_asked_once_per_playback() {
    for a in [hijacked(), already_routed()] {
        let mut w = world_where(Some(a.clone()), here);
        assert_eq!(w.requests().len(), 1, "{a:?}");
        for _ in 0..10 {
            w.advance(secs(BACKOFF + 1.0));
            w.reconcile();
        }
        assert_eq!(w.requests().len(), 1, "{a:?}");
    }
}

#[test]
fn headset_here_and_held_or_refused_is_asked_again_after_the_backoff() {
    for a in [on_a_call(), answer(0, "Something final")] {
        let mut w = world_where(Some(a.clone()), here);
        for _ in 0..10 {
            w.advance(secs(BACKOFF + 1.0));
            w.reconcile();
        }
        assert_eq!(w.requests().len(), 11, "{a:?}");
        assert_eq!(w.count("reclaim:"), 1, "{a:?}");
    }
}

#[test]
fn too_soon_after_a_manual_route_backs_off() {
    let mut w = world(Some(answer(0, "Too soon since last manual route")));
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(BACKOFF));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
}

#[test]
fn hijack_block_period_retries_soon_but_other_final_refusals_still_back_off() {
    let mut w = world(Some(answer(0, "Hijack Block Period")));
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    for reason in ["Not tipi device", "Too soon since last manual route"] {
        let mut w = world(Some(answer(0, reason)));
        w.advance(secs(DELAY + 1.0));
        w.reconcile();
        assert_eq!(w.requests().len(), 1, "{reason}");
    }
}

#[test]
fn hijack_block_period_timeline() {
    let mut w = world(Some(answer(0, "Hijack Block Period")));
    let mut times = vec![0u64];
    println!("t=0s request #1");
    let mut seen = 1;
    for step in 1..=(((SPAN + BACKOFF * 3.0) / DELAY) as u64) {
        w.advance(secs(DELAY));
        w.reconcile();
        let t = step * DELAY as u64;
        if w.requests().len() > seen {
            seen = w.requests().len();
            times.push(t);
            println!("t={t}s request #{seen}");
        }
    }
    // Every 8 s from t=0 while the 180 s window is open; the last one is sent at the first
    // tick after the window closes (the window is checked when the refusal comes back).
    let last_in_window = times.iter().position(|&t| t as f64 >= SPAN).unwrap();
    let expect: Vec<u64> = (0..=last_in_window as u64).map(|i| i * DELAY as u64).collect();
    assert_eq!(&times[..=last_in_window], &expect[..]);
    assert_eq!(times[last_in_window - 1] as f64, SPAN - 4.0);
    // After the window closes, only the 60 s back-off spaces them.
    for pair in times[last_in_window..].windows(2) {
        assert!(pair[1] - pair[0] >= BACKOFF as u64, "{times:?}");
    }
    assert!(times.len() > last_in_window + 1, "back-off still retries: {times:?}");
}

#[test]
fn headset_here_but_not_chosen_is_asked_again_next_playback() {
    let mut w = world_where(Some(hijacked()), here);
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.set(|s| s.output_running = false);
    w.reconcile();
    w.set(|s| s.output_running = true);
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
}

#[test]
fn headset_here_but_not_chosen_retries_short_refusals_within_the_window_then_backs_off() {
    let mut w = world_where(Some(out_of_ear()), here);
    w.advance(secs(DELAY - 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
    w.advance(secs(1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
    for _ in 0..(SPAN / (DELAY + 0.5)) as usize + 1 {
        w.advance(secs(DELAY + 0.5));
        w.reconcile();
    }
    let window_retries = (SPAN / DELAY) as usize + 2;
    assert!(w.requests().len() <= window_retries);
    let spent = w.requests().len();
    w.advance(secs(DELAY + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), spent);
    w.advance(secs(BACKOFF));
    w.reconcile();
    assert_eq!(w.requests().len(), spent + 1);
    assert_eq!(w.count("out of ear"), 1);
}

#[test]
fn headset_becoming_the_output_is_not_asked_for_and_ends_the_spell() {
    let mut w = world(Some(out_of_ear()));
    w.advance(secs(SPAN + 20.0));
    w.set(on_headset);
    for _ in 0..5 {
        w.advance(secs(BACKOFF + 1.0));
        w.reconcile();
    }
    let asked = w.requests().len();
    w.set(|s| {
        s.default_output = Some(mac_studio_speakers().id);
        s.output_running = false;
    });
    w.reconcile();
    w.set(|s| s.output_running = true);
    w.reconcile();
    assert_eq!(w.requests().len(), asked + 1);
    assert_eq!(w.count("refused (Buds out of ear)"), 2);
}

#[test]
fn headset_switched_away_from_mid_playback_is_not_asked_for() {
    let mut w = world_where(Some(hijacked()), on_headset);
    w.set(|s| s.default_output = Some(mac_studio_speakers().id));
    for _ in 0..10 {
        w.advance(secs(BACKOFF + 1.0));
        w.reconcile();
    }
    assert!(w.requests().is_empty());
}

#[test]
fn headset_taken_by_the_phone_mid_playback_is_still_asked_for() {
    let mut w = world_where(None, on_headset);
    w.set(|s| {
        s.devices = vec![mac_studio_speakers()];
        s.default_output = Some(mac_studio_speakers().id);
    });
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    w.advance(secs(INTERVAL + 1.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 2);
}

#[test]
fn headset_switched_away_from_is_asked_again_next_playback() {
    let mut w = world_where(Some(hijacked()), on_headset);
    w.set(|s| s.default_output = Some(mac_studio_speakers().id));
    w.reconcile();
    assert_eq!(w.requests().len(), 0);
    w.set(|s| s.output_running = false);
    w.reconcile();
    w.set(|s| s.output_running = true);
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}
