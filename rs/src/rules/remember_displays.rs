//! Remembers where each display sits and puts the whole set back when macOS rearranges it (Swift
//! `RememberDisplaysRule`).
//!
//! macOS files an arrangement under the exact set of displays that were online, so a set it has not
//! seen before (or one it has forgotten) comes back in a default layout. The last arrangement the
//! user set is what this restores. A difference that appears long after Cleat or a reconnect is the
//! user dragging a display, and becomes the new memory.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::display::{DisplayOrigin, DisplaySnapshot};
use crate::model::Action;

/// How long after Cleat's last write, or after the set of displays changed, a difference is still
/// taken for macOS settling rather than for the user.
pub const USER_MOVE_WINDOW: Duration = Duration::from_secs(10);

/// Each display's top left, by UUID.
pub type Origins = BTreeMap<String, [i32; 2]>;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Remembered and current agree.
    None,
    /// Becomes the memory for this set: the first sighting, or the user's own drag.
    Remember(Origins, String),
    /// Put the whole set back: an `Action::SetDisplayOrigins`.
    Restore(Action),
    /// A whole log line: the set cannot be told apart, so it is neither remembered nor restored.
    Skipped(String),
}

/// The set's key: its UUIDs sorted and joined, so the order macOS lists them in does not matter.
pub fn key(displays: &[DisplaySnapshot]) -> String {
    let mut uuids: Vec<&str> = displays.iter().map(|d| d.uuid.as_str()).collect();
    uuids.sort_unstable();
    uuids.join("|")
}

/// Where each display's top left is, by UUID.
pub fn origins(displays: &[DisplaySnapshot]) -> Origins {
    displays.iter().map(|d| (d.uuid.clone(), d.origin())).collect()
}

/// `since_anchor` is the time since the later of Cleat's last write and the last change of the
/// set; None when neither has happened. `unresolved` is a restore that failed or was given up on:
/// the difference is then still macOS's, however long ago, and is never learned as the user's.
pub fn decide(
    displays: &[DisplaySnapshot],
    remembered: Option<&Origins>,
    combination_changed: bool,
    since_anchor: Option<Duration>,
    unresolved: bool,
) -> Decision {
    let uuids: BTreeSet<&str> = displays.iter().map(|d| d.uuid.as_str()).collect();
    if uuids.len() != displays.len() {
        return Decision::Skipped("rememberDisplays skipped: two displays report the same UUID".into());
    }
    if displays.iter().any(|d| d.uuid.starts_with("unknown-")) {
        return Decision::Skipped("rememberDisplays skipped: a display has no UUID macOS can name it by".into());
    }

    let current = origins(displays);
    // A memory for another set of UUIDs (a file edited by hand) is no memory for this one.
    let Some(remembered) = remembered.filter(|r| r.keys().eq(current.keys())) else {
        return Decision::Remember(
            current,
            format!("first time this set of {} displays is seen", displays.len()),
        );
    };
    if *remembered == current {
        return Decision::None;
    }
    if !combination_changed && !unresolved && since_anchor.is_none_or(|s| s >= USER_MOVE_WINDOW) {
        return Decision::Remember(current, "moved by hand".into());
    }
    Decision::Restore(restore_action(displays, remembered))
}

fn describe(display: &DisplaySnapshot, place: [i32; 2]) -> String {
    format!("{} {} @{},{}", display.name, display.uuid.chars().take(8).collect::<String>(), place[0], place[1])
}

fn restore_action(displays: &[DisplaySnapshot], wanted: &Origins) -> Action {
    // The ids are this connection's: they change across a reconnect, the UUIDs do not.
    let mut ordered: Vec<&DisplaySnapshot> = displays.iter().collect();
    ordered.sort_by_key(|d| d.id);
    let targets: Vec<DisplayOrigin> = ordered
        .iter()
        .map(|d| {
            let place = wanted[&d.uuid];
            DisplayOrigin { id: d.id, x: place[0], y: place[1] }
        })
        .collect();
    let moved: Vec<&&DisplaySnapshot> = ordered.iter().filter(|d| wanted[&d.uuid] != d.origin()).collect();

    let mut reason = format!(
        "{} displays: {} (was {})",
        displays.len(),
        moved.iter().map(|d| describe(d, wanted[&d.uuid])).collect::<Vec<_>>().join(" | "),
        moved.iter().map(|d| describe(d, d.origin())).collect::<Vec<_>>().join(" | "),
    );
    // The display at 0,0 is the main one: restoring a set whose 0,0 is not the current main
    // display changes which screen holds the menu bar.
    if let Some(first) = displays.iter().find(|d| wanted[&d.uuid] == [0, 0]) {
        if !first.is_main {
            reason += &format!(", main display changes to {}", first.name);
        }
    }
    Action::SetDisplayOrigins(targets, reason)
}
