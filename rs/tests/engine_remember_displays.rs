//! `rememberDisplays` end to end (Swift `EngineRememberDisplaysTests`): what is remembered, when it
//! is put back, and when a difference is the user's. The clock is a value, because the 10 second
//! window is the whole judgement.

mod common;

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use cleat_rs::config::Config;
use cleat_rs::display::DisplaySnapshot;
use cleat_rs::engine::{Event, DISPLAY_PLACEMENT_LIMIT};
use cleat_rs::model::DeviceSnapshot;
use cleat_rs::rules::remember_displays::{key, origins};
use cleat_rs::state::arrangements::{read, Arrangements, Loaded};
use common::displays::*;
use common::engine::{Harness, Opts};
use common::mac_speakers;

fn on() -> Config {
    Config { remember_displays: true, launch_at_login: false, ..Config::default() }
}

fn memory(sets: &[&[DisplaySnapshot]]) -> String {
    let a: Arrangements = sets.iter().map(|d| (key(d), origins(d))).collect();
    serde_json::to_string(&a).unwrap()
}

fn start_with(displays: Vec<DisplaySnapshot>, config: &Config, file: Option<String>, honors: bool, succeeds: bool) -> Harness {
    Harness::new(
        config,
        DeviceSnapshot { devices: vec![mac_speakers()], ..Default::default() },
        Opts {
            displays,
            arrangements_file: file,
            display_honors_writes: honors,
            display_write_succeeds: succeeds,
            ..Opts::default()
        },
    )
}

fn start(displays: Vec<DisplaySnapshot>, config: &Config, file: Option<String>) -> Harness {
    start_with(displays, config, file, true, true)
}

fn seeded(displays: Vec<DisplaySnapshot>) -> Harness {
    start(displays, &on(), Some(memory(&[&placed()])))
}

/// Seeded with `placed`, and macOS undoes every write.
fn seeded_undone(displays: Vec<DisplaySnapshot>) -> Harness {
    start_with(displays, &on(), Some(memory(&[&placed()])), false, true)
}

/// Seeded with `placed`, and every write fails.
fn seeded_failing(displays: Vec<DisplaySnapshot>) -> Harness {
    start_with(displays, &on(), Some(memory(&[&placed()])), true, false)
}

fn file(h: &Harness) -> PathBuf {
    h.dir.join("display-arrangements.json")
}

fn remembered(h: &Harness) -> Option<Arrangements> {
    match read(&file(h)) {
        Loaded::Loaded(a) => Some(a),
        _ => None,
    }
}

fn only(displays: &[DisplaySnapshot]) -> Option<Arrangements> {
    Some([(key(displays), origins(displays))].into_iter().collect())
}

fn pass(h: &mut Harness) {
    h.engine.reconcile_displays(true);
}

fn set_and_pass(h: &mut Harness, list: Vec<DisplaySnapshot>) {
    *h.displays.displays.borrow_mut() = list;
    pass(h);
}

fn secs(h: &Harness, s: u64) {
    h.advance(Duration::from_secs(s));
}

fn batches(h: &Harness) -> Vec<String> {
    h.displays.batches.borrow().clone()
}

fn lines(h: &Harness, needle: &str) -> Vec<String> {
    h.log_lines().into_iter().filter(|l| l.contains(needle)).collect()
}

fn restores(h: &Harness) -> Vec<String> {
    lines(h, "restoreDisplays ").into_iter().filter(|l| l.contains("(was")).collect()
}

fn origins_now(h: &Harness) -> cleat_rs::rules::remember_displays::Origins {
    origins(&h.displays.displays.borrow())
}

// MARK: Remembering

#[test]
fn first_set_is_remembered_and_nothing_moves() {
    let h = start(docked(), &on(), None);
    assert!(batches(&h).is_empty());
    assert_eq!(remembered(&h), only(&docked()));
    assert_eq!(lines(&h, "rememberDisplays saved 5 displays").len(), 1);
    assert!(h.displays.observing.get());
}

#[test]
fn a_matching_arrangement_is_left_alone_and_not_rewritten() {
    let mut h = seeded(placed());
    let before = std::fs::read(file(&h)).unwrap();
    for _ in 0..3 {
        pass(&mut h);
    }
    assert!(batches(&h).is_empty());
    assert!(lines(&h, "rememberDisplays saved").is_empty());
    assert_eq!(std::fs::read(file(&h)).unwrap(), before);
}

// MARK: Restoring

#[test]
fn the_first_pass_after_launch_restores_the_remembered_arrangement() {
    let h = seeded(scrambled());
    assert_eq!(batches(&h), vec![batch(&placed())]);
    assert_eq!(origins_now(&h), origins(&placed()));
    assert_eq!(restores(&h).len(), 1);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn a_reconnected_set_is_restored() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 20);
    set_and_pass(&mut h, four()); // the laptop leaves: a new set, remembered
    secs(&h, 20);
    set_and_pass(&mut h, scrambled()); // and comes back wherever macOS put it
    assert_eq!(batches(&h), vec![batch(&placed())]);
}

#[test]
fn a_reconnect_listed_in_another_order_still_finds_its_memory() {
    let mut reversed = scrambled();
    reversed.reverse();
    let h = seeded(reversed);
    assert_eq!(batches(&h).len(), 1);
    assert_eq!(remembered(&h).map(|a| a.len()), Some(1));
}

#[test]
fn the_event_caused_by_the_restore_ends_there() {
    let mut h = seeded(scrambled());
    // start, the event's own look at the set, and the pass the debounce runs.
    h.drive(2000);
    assert_eq!(batches(&h).len(), 1);
    assert_eq!(h.displays.snapshot_calls.get(), 3);
    h.drive(30_000);
    assert_eq!(batches(&h).len(), 1);
    assert_eq!(h.displays.snapshot_calls.get(), 3);
}

#[test]
fn the_restore_is_one_transaction() {
    let h = seeded(scrambled());
    assert_eq!(batches(&h).len(), 1);
    assert_eq!(origins_now(&h), origins(&placed()));
}

// MARK: The user's hand

#[test]
fn a_hand_move_after_the_window_is_remembered() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 11);
    set_and_pass(&mut h, docked());
    assert!(batches(&h).is_empty());
    assert_eq!(remembered(&h), only(&docked()));
    assert_eq!(lines(&h, "moved by hand").len(), 1);
}

#[test]
fn after_a_hand_move_the_new_place_is_what_gets_restored() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 11);
    set_and_pass(&mut h, docked());
    secs(&h, 20);
    set_and_pass(&mut h, four());
    secs(&h, 20);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h), vec![batch(&docked())]);
}

#[test]
fn a_change_inside_the_window_after_a_restore_is_undone_not_remembered() {
    let mut h = seeded(scrambled());
    secs(&h, 9);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h).len(), 2);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn nine_seconds_after_a_restore_is_still_macos() {
    let mut h = seeded(scrambled());
    secs(&h, 9);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h).len(), 2);
    assert!(lines(&h, "rememberDisplays saved").is_empty());
}

#[test]
fn eleven_seconds_after_a_restore_is_the_user() {
    let mut h = seeded(scrambled());
    secs(&h, 11);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h).len(), 1);
    assert_eq!(lines(&h, "moved by hand").len(), 1);
    let expected: Arrangements = [(key(&placed()), origins(&scrambled()))].into_iter().collect();
    assert_eq!(remembered(&h), Some(expected));
}

/// macOS settling a few seconds after a reconnect that Cleat had nothing to correct is not the
/// user, even though Cleat has written nothing for a long time.
#[test]
fn macos_settling_right_after_a_reconnect_is_not_taken_for_the_user() {
    let mut h = seeded(placed());
    secs(&h, 5);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h).len(), 1);
    assert!(lines(&h, "moved by hand").is_empty());
    assert_eq!(remembered(&h), only(&placed()));
}

// MARK: Not fighting macOS

#[test]
fn stops_after_three_restores_when_macos_keeps_undoing_them() {
    let mut h = seeded_undone(scrambled());
    for _ in 0..5 {
        secs(&h, 3);
        pass(&mut h);
    }
    assert_eq!(batches(&h).len() as u32, DISPLAY_PLACEMENT_LIMIT);
    assert_eq!(lines(&h, "restoreDisplays gave up").len(), 1);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn a_new_combination_gives_the_restore_three_more_tries() {
    let mut h = seeded_undone(scrambled());
    for _ in 0..4 {
        secs(&h, 3);
        pass(&mut h);
    }
    assert_eq!(batches(&h).len(), 3);

    secs(&h, 3);
    set_and_pass(&mut h, four());
    secs(&h, 3);
    set_and_pass(&mut h, scrambled());
    assert_eq!(batches(&h).len(), 4);
}

#[test]
fn a_failed_restore_is_logged_and_counted() {
    let mut h = seeded_failing(scrambled());
    let failed = restores(&h);
    assert_eq!(failed.len(), 1);
    assert!(failed[0].ends_with(" failed"), "{}", failed[0]);

    secs(&h, 8);
    pass(&mut h);
    assert_eq!(batches(&h).len(), 2);
}

#[test]
fn an_unreadable_display_list_changes_nothing() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 20);
    set_and_pass(&mut h, vec![]);
    assert!(batches(&h).is_empty());
    assert_eq!(lines(&h, "rememberDisplays saved").len(), 1);
}

// MARK: A restore that did not take, and a set that came and went

#[test]
fn a_short_unplug_merged_by_the_debounce_is_still_a_new_combination() {
    let mut h = seeded(placed());
    secs(&h, 20);
    *h.displays.displays.borrow_mut() = four();
    h.engine.handle(Event::DisplaysChanged);
    *h.displays.displays.borrow_mut() = scrambled();
    h.engine.handle(Event::DisplaysChanged);
    pass(&mut h); // one pass sees the same set again

    assert_eq!(batches(&h), vec![batch(&placed())]);
    assert!(lines(&h, "moved by hand").is_empty());
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn a_failed_restore_is_not_overwritten_by_the_memory_after_the_window() {
    let mut h = seeded_failing(scrambled());
    secs(&h, 11);
    pass(&mut h);

    assert!(lines(&h, "moved by hand").is_empty());
    assert_eq!(batches(&h).len(), 2);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn a_given_up_restore_is_not_overwritten_by_the_memory_after_the_window() {
    let mut h = seeded_undone(scrambled());
    for _ in 0..3 {
        secs(&h, 3);
        pass(&mut h);
    }
    assert_eq!(lines(&h, "restoreDisplays gave up").len(), 1);
    secs(&h, 11);
    pass(&mut h);

    assert!(lines(&h, "moved by hand").is_empty());
    assert_eq!(batches(&h).len(), 3);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn an_unresolved_restore_is_forgiven_when_the_arrangement_matches_again() {
    let mut h = seeded_undone(scrambled());
    for _ in 0..3 {
        secs(&h, 3);
        pass(&mut h);
    }
    // The user (or macOS) puts it right; later a hand move counts again.
    secs(&h, 20);
    set_and_pass(&mut h, placed());
    secs(&h, 20);
    set_and_pass(&mut h, docked());
    assert_eq!(lines(&h, "moved by hand").len(), 1);
    assert_eq!(remembered(&h), only(&docked()));
}

// MARK: Settings

#[test]
fn with_the_setting_off_the_file_is_never_touched() {
    let mut h = start(docked(), &Config { launch_at_login: false, ..Config::default() }, None);
    for _ in 0..3 {
        pass(&mut h);
    }
    secs(&h, 20);
    set_and_pass(&mut h, four());
    assert!(!file(&h).exists());
    assert!(lines(&h, "rememberDisplays").is_empty());
    assert_eq!(h.rule("rememberDisplays"), "off");
}

#[test]
fn turning_it_on_remembers_the_current_arrangement() {
    let mut h = start(docked(), &Config { launch_at_login: false, ..Config::default() }, None);
    assert!(!file(&h).exists());

    std::fs::write(h.dir.join("config.json"), serde_json::to_string(&on()).unwrap()).unwrap();
    h.engine.handle(Event::ConfigTouched { received: SystemTime::now() });
    h.drive(300);

    assert_eq!(remembered(&h), only(&docked()));
    assert!(batches(&h).is_empty());
}

// MARK: Persistence and status

#[test]
fn a_remembered_arrangement_outlives_the_daemon() {
    let mut first = start(placed(), &on(), None);
    secs(&first, 11);
    set_and_pass(&mut first, docked()); // the user's last word

    let text = std::fs::read_to_string(file(&first)).unwrap();
    let second = start(scrambled(), &on(), Some(text));
    assert_eq!(batches(&second), vec![batch(&docked())]);
}

#[test]
fn an_unreadable_memory_file_starts_empty_with_one_line() {
    let h = start(placed(), &on(), Some("{not json".into()));
    assert_eq!(lines(&h, "rememberDisplays: could not read").len(), 1);
    assert_eq!(remembered(&h), only(&placed()));
}

#[test]
fn status_says_how_many_sets_are_remembered() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 20);
    set_and_pass(&mut h, four());
    assert_eq!(h.rule("rememberDisplays"), "on (2 arrangements remembered)");
}

#[test]
fn saved_is_written_only_when_the_content_changes() {
    let mut h = start(placed(), &on(), None);
    secs(&h, 20);
    set_and_pass(&mut h, docked());
    for _ in 0..2 {
        secs(&h, 20);
        pass(&mut h);
    }
    assert_eq!(lines(&h, "moved by hand").len(), 1);
    assert_eq!(lines(&h, "rememberDisplays saved").len(), 2); // first sight, then the hand
}
