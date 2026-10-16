//! `rememberDisplays`' rule (Swift `RememberDisplaysRuleTests`), case for case.

mod common;

use std::time::Duration;

use cleat_rs::display::{DisplayOrigin, DisplaySnapshot};
use cleat_rs::model::Action;
use cleat_rs::rules::remember_displays::{decide, key, origins, Decision, Origins, USER_MOVE_WINDOW};
use common::displays::*;

fn secs(s: f64) -> Option<Duration> {
    Some(Duration::from_secs_f64(s))
}

fn run(displays: &[DisplaySnapshot], remembered: Option<&Origins>, changed: bool, since: Option<Duration>) -> Decision {
    decide(displays, remembered, changed, since, false)
}

fn restored(decision: &Decision) -> Option<(&Vec<DisplayOrigin>, &String)> {
    match decision {
        Decision::Restore(Action::SetDisplayOrigins(o, r)) => Some((o, r)),
        _ => None,
    }
}

fn is_remember(decision: &Decision, expected: &Origins, reason: Option<&str>) -> bool {
    match decision {
        Decision::Remember(o, why) => o == expected && reason.is_none_or(|r| why.contains(r)),
        _ => false,
    }
}

fn o(id: u32, x: i32, y: i32) -> DisplayOrigin {
    DisplayOrigin { id, x, y }
}

// MARK: The table

#[test]
fn first_sight_of_a_set_is_remembered_as_it_is() {
    let decision = run(&docked(), None, true, None);
    assert!(is_remember(&decision, &origins(&docked()), Some("first time")), "{decision:?}");
}

#[test]
fn matching_arrangement_does_nothing() {
    let placed_origins = origins(&placed());
    for changed in [true, false] {
        for since in [0.0, 100.0] {
            assert_eq!(run(&placed(), Some(&placed_origins), changed, secs(since)), Decision::None);
        }
    }
}

#[test]
fn a_newly_joined_set_that_differs_is_restored() {
    let decision = run(&docked(), Some(&origins(&placed())), true, secs(100.0));
    let (targets, _) = restored(&decision).unwrap_or_else(|| panic!("{decision:?}"));
    assert_eq!(
        *targets,
        vec![o(1, 0, 0), o(2, 2560, 0), o(3, 0, -1440), o(4, 2560, -1440), o(5, 1696, 1440)]
    );
}

#[test]
fn a_difference_outside_the_window_is_the_users_and_is_remembered() {
    let decision = run(&docked(), Some(&origins(&placed())), false, secs(11.0));
    assert!(is_remember(&decision, &origins(&docked()), Some("moved by hand")), "{decision:?}");
}

#[test]
fn a_difference_with_no_anchor_is_the_users() {
    let decision = run(&docked(), Some(&origins(&placed())), false, None);
    assert!(is_remember(&decision, &origins(&docked()), None));
}

#[test]
fn a_difference_inside_the_window_is_restored() {
    assert!(restored(&run(&docked(), Some(&origins(&placed())), false, secs(9.0))).is_some());
}

#[test]
fn the_window_edge_belongs_to_the_user() {
    let remembered = origins(&placed());
    assert!(is_remember(&run(&docked(), Some(&remembered), false, Some(USER_MOVE_WINDOW)), &origins(&docked()), None));
    let just_inside = USER_MOVE_WINDOW - Duration::from_millis(1);
    assert!(restored(&run(&docked(), Some(&remembered), false, Some(just_inside))).is_some());
}

// MARK: Keys, ids, reason

#[test]
fn the_key_ignores_the_order_displays_are_listed_in() {
    let mut reversed = placed();
    reversed.reverse();
    assert_eq!(key(&placed()), key(&reversed));
    let mut uuids: Vec<String> = placed().iter().map(|d| d.uuid.clone()).collect();
    uuids.sort();
    assert_eq!(key(&placed()), uuids.join("|"));
}

#[test]
fn restore_uses_the_ids_of_this_connection() {
    let renumbered: Vec<DisplaySnapshot> =
        docked().into_iter().enumerate().map(|(i, d)| DisplaySnapshot { id: 11 + i as u32, ..d }).collect();
    let decision = run(&renumbered, Some(&origins(&placed())), true, None);
    let (targets, _) = restored(&decision).unwrap();
    assert_eq!(targets.iter().map(|t| t.id).collect::<Vec<_>>(), vec![11, 12, 13, 14, 15]);
    assert_eq!(targets.last(), Some(&o(15, 1696, 1440)));
}

#[test]
fn restore_reason_lists_only_the_displays_that_moved() {
    let decision = run(&docked(), Some(&origins(&placed())), true, None);
    let (_, reason) = restored(&decision).unwrap();
    assert!(reason.contains("Built-in Retina Display 37D8832A @1696,1440"), "{reason}");
    assert!(reason.contains("(was Built-in Retina Display 37D8832A @5120,0)"), "{reason}");
    assert!(!reason.contains("DELL U3223QE AAAAAAAA"), "{reason}");
}

#[test]
fn restore_says_when_the_main_display_changes() {
    // Remembered with the laptop at 0,0, which it is not now.
    let mut remembered = origins(&placed());
    remembered.insert(LAPTOP_UUID.into(), [0, 0]);
    remembered.insert(DELL_A_UUID.into(), [1728, 0]);
    let decision = run(&placed(), Some(&remembered), true, None);
    let (_, reason) = restored(&decision).unwrap();
    assert!(reason.contains("main display changes to Built-in Retina Display"), "{reason}");
}

#[test]
fn a_size_change_alone_is_not_a_difference() {
    let resized = screen(5, "Built-in Retina Display", LAPTOP_UUID, (1696, 1440, 1512, 982), true, false);
    let mut displays = placed();
    displays.pop();
    displays.push(resized);
    assert_eq!(run(&displays, Some(&origins(&placed())), false, secs(100.0)), Decision::None);
}

#[test]
fn a_memory_for_another_set_of_uuids_is_no_memory() {
    let mut other = origins(&placed());
    let moved = other.remove(DELL_A_UUID).unwrap();
    other.insert("EEEEEEEE-0000-0000-0000-000000000009".into(), moved);
    assert!(is_remember(&run(&placed(), Some(&other), true, None), &origins(&placed()), None));
}

// MARK: Sets that cannot be told apart

#[test]
fn duplicate_uuids_are_skipped() {
    let mut displays = placed();
    displays.push(screen(6, "DELL U3223QE", DELL_A_UUID, (9000, 0, 2560, 1440), false, false));
    match run(&displays, None, false, None) {
        Decision::Skipped(line) => assert!(line.contains("same UUID"), "{line}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unknown_uuid_is_skipped() {
    let mut displays = placed();
    displays.push(screen(6, "Mystery", "unknown-7", (9000, 0, 100, 100), false, false));
    assert!(matches!(run(&displays, None, false, None), Decision::Skipped(_)));
}

// MARK: A restore that did not take

#[test]
fn an_unresolved_restore_keeps_restoring_outside_the_window() {
    let remembered = origins(&placed());
    let decision = decide(&docked(), Some(&remembered), false, secs(11.0), true);
    assert!(restored(&decision).is_some(), "{decision:?}");
    let decision = decide(&docked(), Some(&remembered), false, None, true);
    assert!(restored(&decision).is_some(), "{decision:?}");
    // Agreement is still agreement.
    assert_eq!(decide(&placed(), Some(&remembered), false, secs(11.0), true), Decision::None);
}
