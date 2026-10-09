//! Rule 7 following the person, not the screen (B-1173). Swift `ReclaimPresenceTests.swift` ported
//! with the new judgement: keyboard in the last 30 s, or the front app holding a video wake lock;
//! a display held awake by anything else, or a reading that failed, is nobody at the Mac.

mod common;

use std::time::Duration;

use cleat_rs::audio::ListenerKind;
use cleat_rs::engine::Event;
use cleat_rs::model::presence::{judge, DisplayAssertion, PresenceFacts};
use cleat_rs::model::DeviceSnapshot;
use common::engine::Opts;
use common::reclaim::*;
use common::*;

const CHROME: i32 = 21981;
const ZOOM: i32 = 4242;
const OTHER_FRONT: i32 = 777;

fn chrome_video() -> DisplayAssertion {
    // pmset -g assertions, 2026-10-07 19:28: pid 21981(Google Chrome) NoDisplaySleepAssertion
    // named: "Video Wake Lock".
    DisplayAssertion { pid: CHROME, process: "Google Chrome".into(), name: "Video Wake Lock".into() }
}
fn meeting() -> DisplayAssertion {
    // Synthetic: no Zoom/Keynote sample was available; any non-video display assertion.
    DisplayAssertion { pid: ZOOM, process: "zoom.us".into(), name: "Zoom Meeting".into() }
}

fn facts(idle: Option<f64>, assertions: Vec<DisplayAssertion>, front: Option<i32>) -> PresenceFacts {
    PresenceFacts { input_idle: idle, display_assertions: Some(assertions), front_pid: front }
}

/// Playing on the AirPods, with the wired headphones as the other output in CoreAudio.
fn on_the_headset(snap: &mut DeviceSnapshot) {
    snap.devices = vec![air_pods(), wired_headphones()];
    snap.default_output = Some(air_pods().id);
}

fn move_output(w: &common::engine::Harness, to: u32) {
    w.set(|s| s.default_output = Some(to));
}

// MARK: - The four cells the addendum names

#[test]
fn display_held_awake_with_idle_keyboard_is_nobody() {
    let w = world_where(Some(hijacked()), |s| s.presence = facts(Some(45.0), vec![meeting()], Some(ZOOM)));
    assert!(w.requests().is_empty());
    assert_eq!(
        w.count("reclaim: AirPods Max not asked (nobody at the Mac (keyboard idle 45 s, no foreground video; display held awake by zoom.us \"Zoom Meeting\"))"),
        1
    );
}

#[test]
fn keyboard_in_the_last_30_seconds_is_someone() {
    let w = world_where(Some(hijacked()), |s| s.presence = facts(Some(29.0), vec![], None));
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

#[test]
fn idle_keyboard_with_a_foreground_video_is_someone() {
    let w = world_where(Some(hijacked()), |s| s.presence = facts(Some(120.0), vec![chrome_video()], Some(CHROME)));
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    assert_eq!(w.count("nobody at the Mac"), 0);
}

#[test]
fn idle_keyboard_with_a_background_video_is_nobody() {
    let w = world_where(Some(hijacked()), |s| s.presence = facts(Some(120.0), vec![chrome_video()], Some(OTHER_FRONT)));
    assert!(w.requests().is_empty());
    assert_eq!(w.count("not asked (nobody at the Mac (keyboard idle 120 s, video in background app Google Chrome))"), 1);
}

// MARK: - The enumeration table, every cell

#[derive(Clone, Copy, Debug)]
enum Holder {
    None,
    ForegroundVideo,
    ForegroundOther,
    BackgroundApp,
}

#[derive(Clone, Copy, Debug)]
enum Headset {
    OnTheMac,
    PhonePlaying,
    PhoneIdle,
}

/// Idle (<30 / >=30) x display assertion (none / front video / front non-video / background app) x
/// Mac playing x where the headset is: 2 x 4 x 2 x 3 = 48 cells, generated, not picked. Each
/// cell asserts whether a request went out and the one reclaim line it wrote.
#[test]
fn every_cell_of_the_presence_table() {
    let mut cells = 0;
    for idle in [5.0, 45.0] {
        for holder in [Holder::None, Holder::ForegroundVideo, Holder::ForegroundOther, Holder::BackgroundApp] {
            for playing_ in [true, false] {
                for where_ in [Headset::OnTheMac, Headset::PhonePlaying, Headset::PhoneIdle] {
                    cells += 1;
                    let (assertions, front) = match holder {
                        Holder::None => (vec![], Some(OTHER_FRONT)),
                        Holder::ForegroundVideo => (vec![chrome_video()], Some(CHROME)),
                        Holder::ForegroundOther => (vec![meeting()], Some(ZOOM)),
                        Holder::BackgroundApp => (vec![chrome_video()], Some(OTHER_FRONT)),
                    };
                    let answer_ = match where_ {
                        Headset::PhonePlaying => media_hold(),
                        _ => hijacked(),
                    };
                    let w = world_where(Some(answer_), |s| {
                        s.output_running = playing_;
                        s.presence = facts(Some(idle), assertions.clone(), front);
                        if let Headset::OnTheMac = where_ {
                            s.devices = vec![air_pods(), mac_studio_speakers()];
                            s.default_output = Some(air_pods().id);
                        }
                    });
                    let present = idle < 30.0 || matches!(holder, Holder::ForegroundVideo);
                    let asks = playing_ && !matches!(where_, Headset::OnTheMac) && present;
                    let cell = format!("idle {idle} {holder:?} playing {playing_} {where_:?}");
                    assert_eq!(w.requests().len(), usize::from(asks), "{cell}");
                    let lines: Vec<String> = w.log_lines().into_iter().filter(|l| l.contains("reclaim:")).collect();
                    let expected: Vec<String> = if !playing_ || matches!(where_, Headset::OnTheMac) {
                        vec![]
                    } else if asks {
                        vec![match where_ {
                            Headset::PhonePlaying => "reclaim: AirPods Max held by remote device (Remote Category 301 > Local Category 200, audio streaming)".into(),
                            _ => "reclaim: AirPods Max <- remote device (hijack accepted)".into(),
                        }]
                    } else {
                        let video = match holder {
                            Holder::None => "no foreground video".to_string(),
                            Holder::ForegroundOther => "no foreground video; display held awake by zoom.us \"Zoom Meeting\"".into(),
                            Holder::BackgroundApp => "video in background app Google Chrome".into(),
                            Holder::ForegroundVideo => unreachable!(),
                        };
                        vec![format!("reclaim: AirPods Max not asked (nobody at the Mac (keyboard idle 45 s, {video}))")]
                    };
                    let tails: Vec<&str> = lines.iter().map(|l| &l[20..]).collect();
                    assert_eq!(tails, expected, "{cell}");
                }
            }
        }
    }
    assert_eq!(cells, 48);
}

// MARK: - Readings that could not be taken

#[test]
fn unreadable_keyboard_idle_counts_as_nobody() {
    let w = world_where(Some(hijacked()), |s| s.presence = PresenceFacts::default());
    assert!(w.requests().is_empty());
    assert_eq!(w.count("not asked (nobody at the Mac (keyboard idle unknown, display assertions unreadable))"), 1);
}

#[test]
fn unreadable_front_app_counts_as_nobody() {
    let w = world_where(Some(hijacked()), |s| s.presence = facts(Some(60.0), vec![chrome_video()], None));
    assert!(w.requests().is_empty());
    assert_eq!(w.count("(keyboard idle 60 s, foreground app unknown)"), 1);
}

#[test]
fn judge_reads_only_the_keyboard_when_it_is_recent() {
    assert!(judge(&PresenceFacts { input_idle: Some(0.0), ..Default::default() }).is_ok());
    assert!(judge(&PresenceFacts { input_idle: Some(30.0), ..Default::default() }).is_err());
}

#[test]
fn front_pid_parses_both_lsappinfo_forms() {
    use cleat_rs::app::presence::parse_pid;
    assert_eq!(parse_pid("    pid = 21981 !cgsConnection"), Some(21981));
    assert_eq!(parse_pid("\"pid\"=21981\n"), Some(21981));
    assert_eq!(parse_pid("[ NULL ]"), None);
}

// MARK: - Swift ReclaimPresenceTests, with the new judgement

#[test]
fn phone_taking_a_headset_that_stays_listed_is_asked_back_when_the_user_returns() {
    let mut w = world_where(None, on_the_headset);
    w.user(Some(40.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    assert!(w.requests().is_empty());
    w.user(Some(2.0));
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    assert_eq!(*w.routing.scores.borrow(), vec![201]);
}

#[test]
fn idle_user_is_waited_for_on_its_own_beat() {
    let mut w = world_where(None, on_the_headset);
    w.user(Some(40.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    assert!(w.engine.reconcile_pending(8000));
}

#[test]
fn hand_picked_output_stands_for_the_playback() {
    let mut w = world_where(None, on_the_headset);
    w.user(Some(1.0));
    move_output(&w, wired_headphones().id);
    for _ in 0..10 {
        w.advance(secs(61.0));
        w.reconcile();
    }
    assert!(w.requests().is_empty());
    assert_eq!(w.count("not asked (the user picked 外接耳機)"), 1);
}

#[test]
fn cleats_own_pin_is_not_the_users_choice() {
    let mut config = reclaim_config(&["AirPods Max"]);
    config.output = s(&["外接耳機", "AirPods Max"]);
    let mut snap = playing();
    on_the_headset(&mut snap);
    let mut w = world_with(config, None, vec![connected_air_pods()], snap);
    w.reconcile();
    assert!(w.writes().contains(&format!("output:{}", wired_headphones().id)));
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

#[test]
fn hand_picked_choice_ends_when_the_headset_is_the_output_again() {
    let mut w = world_where(None, on_the_headset);
    w.user(Some(1.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    move_output(&w, air_pods().id);
    w.reconcile();
    w.user(Some(40.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    w.user(Some(2.0));
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

#[test]
fn idle_user_is_not_asked_for_and_says_so_once() {
    let mut w = world_where(None, |s| s.presence = facts(Some(120.0), vec![], None));
    for i in 0..5 {
        w.advance(secs(8.0));
        // The idle seconds grow every beat; the line is still written once.
        w.set(|s| s.presence.input_idle = Some(128.0 + 8.0 * i as f64));
        w.reconcile();
    }
    assert!(w.requests().is_empty());
    assert_eq!(w.count("not asked (nobody at the Mac"), 1);
    w.user(Some(3.0));
    w.reconcile();
    assert_eq!(w.requests().len(), 1);
}

/// Swift 0.3.11 counted any display held awake; Rust counts only the front app's video.
#[test]
fn video_in_the_front_app_counts_as_using_the_mac() {
    let w = world_where(None, |s| s.presence = facts(Some(120.0), vec![chrome_video()], Some(CHROME)));
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
    assert_eq!(w.count("nobody at the Mac"), 0);
}

// MARK: - The 17:23 record (tests/fixtures/swift-cleat-1720-1740.log)

/// 2026-10-07 17:23: the AirPods connected and became the output at 17:23:06, the phone then took
/// them (they stayed in CoreAudio, the output fell to the wired headphones) and the log shows zero
/// reclaim lines up to 17:36. What the log cannot say is reconstructed and named: the last
/// keyboard event is taken as 17:23:06 (the pin), the phone takes the headset at 17:23:40, and a
/// meeting app holds the display awake - the case 0.3.11 would have answered by asking. Every
/// later line in the record is a reconcile beat (output volume changes and device changes are);
/// each is replayed with the idle time that has grown since.
#[test]
fn replay_of_the_1723_record() {
    let record = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/swift-cleat-1720-1740.log")).unwrap();
    let at = |line: &str| -> f64 {
        let t = &line[11..19];
        let n: Vec<f64> = t.split(':').map(|x| x.parse().unwrap()).collect();
        n[0] * 3600.0 + n[1] * 60.0 + n[2]
    };
    let last_keyboard = at("2026-10-07 17:23:06");
    let taken = at("2026-10-07 17:23:40");
    let beats: Vec<f64> = record.lines().map(at).filter(|t| *t > taken).collect();
    assert_eq!(beats.len(), 5, "17:35:04, 17:35:05, 17:35:40, 17:38:21, 17:38:23");

    let mut snap = playing();
    on_the_headset(&mut snap);
    snap.presence = facts(Some(0.0), vec![meeting()], Some(ZOOM));
    let mut w = world_with(reclaim_config(&["AirPods Max"]), Some(hijacked()), vec![connected_air_pods()], snap);
    assert!(w.requests().is_empty(), "17:23:06 the AirPods are the output");

    let idle_at = |t: f64| t - last_keyboard;
    for t in std::iter::once(taken).chain(beats.iter().copied()) {
        w.set(|s| {
            s.default_output = Some(wired_headphones().id);
            s.presence = facts(Some(idle_at(t)), vec![meeting()], Some(ZOOM));
        });
        w.reconcile();
        // What 0.3.11 decided here: idle < 30 or any display held awake. It would have asked.
        assert!(idle_at(t) >= 30.0 && !w.status().rules.is_empty());
        assert!(w.requests().is_empty(), "beat {t}: nobody at the Mac, nothing asked");
    }
    let lines: Vec<String> = w.log_lines().into_iter().filter(|l| l.contains("reclaim:")).collect();
    assert_eq!(lines.len(), 1, "one line for the whole spell: {lines:?}");
    assert!(lines[0].ends_with(
        "reclaim: AirPods Max not asked (nobody at the Mac (keyboard idle 34 s, no foreground video; display held awake by zoom.us \"Zoom Meeting\"))"
    ));

    // 17:36: back at the keyboard. Asked at once.
    w.user(Some(1.0));
    w.reconcile();
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]));
}

// MARK: - How long the switch back took

fn taken_world(answer_: Option<cleat_rs::reclaim::RouteResponse>) -> common::engine::Harness {
    world_with(
        reclaim_config(&["AirPods Max"]),
        answer_,
        vec![connected_air_pods()],
        playing(),
    )
}

fn arrive(w: &mut common::engine::Harness) {
    w.set(|s| {
        s.devices = vec![mac_studio_speakers(), air_pods()];
        s.default_output = Some(air_pods().id);
    });
    w.fire(ListenerKind::DefaultOutput);
}

#[test]
fn accepted_hijack_logs_the_seconds_to_the_output() {
    let mut w = taken_world(Some(hijacked()));
    w.drive(800);
    w.advance(Duration::from_millis(1000));
    arrive(&mut w);
    assert_eq!(w.count("reclaim: AirPods Max back on the Mac in 1.8 s"), 1);
    w.drive(20_000);
    assert_eq!(w.count("back on the Mac"), 1);
    assert_eq!(w.count("not back"), 0);
}

#[test]
fn headset_arriving_before_the_answer_is_timed_from_the_request() {
    let mut w = taken_world(None);
    w.advance(Duration::from_millis(400));
    arrive(&mut w);
    assert_eq!(w.count("back on the Mac"), 0, "not logged before the hijack is accepted");
    w.engine.handle(Event::RouteAnswered {
        name: "AirPods Max".into(),
        address: AIR_PODS_ADDRESS.into(),
        response: hijacked(),
    });
    assert_eq!(w.count("reclaim: AirPods Max back on the Mac in 0.4 s"), 1);
}

#[test]
fn accepted_hijack_that_never_arrives_says_so_at_ten_seconds() {
    let mut w = taken_world(Some(hijacked()));
    w.drive(9_900);
    assert_eq!(w.count("not back"), 0);
    w.drive(200);
    assert_eq!(w.count("reclaim: AirPods Max not back on the Mac 10 s after the request (not in CoreAudio)"), 1);
}

#[test]
fn refused_request_is_not_timed() {
    let mut w = taken_world(Some(out_of_ear()));
    w.advance(Duration::from_millis(500));
    arrive(&mut w);
    w.drive(20_000);
    assert_eq!(w.count("back on the Mac"), 0);
}

// MARK: - macOS switching the headset back by itself (B-1173, Albert 20:25)

fn nobody_playing() -> common::engine::Harness {
    world_where(None, |s| s.presence = facts(Some(120.0), vec![], None))
}

#[test]
fn macos_switching_back_by_itself_logs_one_line_with_the_seconds() {
    let mut w = nobody_playing();
    assert!(w.requests().is_empty());
    w.advance(Duration::from_millis(1500));
    arrive(&mut w);
    w.drive(20_000);
    assert_eq!(w.count("reclaim: AirPods Max back on the Mac (macOS) in 1.5 s"), 1);
    assert_eq!(w.count("back on the Mac"), 1);
}

#[test]
fn macos_switch_slower_than_the_limit_is_not_logged() {
    for (after, lines) in [(31.0, 0), (29.0, 1)] {
        let mut w = nobody_playing();
        w.advance(secs(after));
        arrive(&mut w);
        w.drive(20_000);
        assert_eq!(w.count("(macOS)"), lines, "{after} s");
    }
}

#[test]
fn cleats_own_request_logs_only_its_own_line() {
    let mut w = taken_world(Some(hijacked()));
    w.drive(800);
    w.advance(Duration::from_millis(1000));
    arrive(&mut w);
    w.drive(20_000);
    assert_eq!(w.count("reclaim: AirPods Max back on the Mac in 1.8 s"), 1);
    assert_eq!(w.count("(macOS)"), 0);
}

#[test]
fn playback_stopping_voids_the_clock() {
    let mut w = nobody_playing();
    w.advance(secs(5.0));
    w.set(|s| s.output_running = false);
    w.reconcile();
    w.advance(secs(5.0));
    w.set(|s| s.output_running = true);
    w.reconcile();
    w.advance(secs(1.0));
    arrive(&mut w);
    assert_eq!(w.count("reclaim: AirPods Max back on the Mac (macOS) in 1.0 s"), 1, "timed from the new playback");

    let mut w = nobody_playing();
    w.set(|s| s.output_running = false);
    w.reconcile();
    arrive(&mut w);
    w.drive(20_000);
    assert_eq!(w.count("back on the Mac"), 0, "arrived after the playback stopped");
}

#[test]
fn headset_moved_away_by_hand_is_not_timed() {
    let mut w = world_where(None, |s| {
        on_the_headset(s);
        s.presence = facts(Some(120.0), vec![], None);
    });
    w.user(Some(1.0));
    move_output(&w, wired_headphones().id);
    w.reconcile();
    w.advance(secs(2.0));
    move_output(&w, air_pods().id);
    w.fire(ListenerKind::DefaultOutput);
    w.drive(20_000);
    assert_eq!(w.count("back on the Mac"), 0);
}

#[test]
fn headset_already_the_output_when_playback_starts_is_not_timed() {
    let mut w = world_where(None, |s| {
        on_the_headset(s);
        s.output_running = false;
    });
    w.set(|s| s.output_running = true);
    w.fire(ListenerKind::Running);
    w.drive(20_000);
    w.fire(ListenerKind::DefaultOutput);
    assert_eq!(w.count("back on the Mac"), 0);
}

// MARK: - The switch kept apart from the list (§6.6 E4, E5)

#[test]
fn reclaim_switched_off_keeps_the_list_and_asks_nothing() {
    let mut config = reclaim_config(&["AirPods Max"]);
    config.reclaim_enabled = Some(false);
    let mut w = common::engine::Harness::new(
        &config,
        playing(),
        Opts { headsets: vec![connected_air_pods()], trace: true, ..Opts::default() },
    );
    assert!(w.requests().is_empty());
    assert_eq!(w.pairings.reads.get(), 0);
    assert_eq!(w.rule("reclaim"), "off");
    assert_eq!(w.count("running@"), 0);

    config.reclaim_enabled = Some(true);
    std::fs::write(w.dir.join("config.json"), serde_json::to_string(&config).unwrap()).unwrap();
    w.engine.handle(Event::ConfigTouched { received: std::time::SystemTime::now() });
    w.drive(400);
    assert!(w.count("running@") >= 1, "the running listener is asked for again");
    assert_eq!(w.requests(), s(&[AIR_PODS_ADDRESS]), "the list survived the switch");
}
