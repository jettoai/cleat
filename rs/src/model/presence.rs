//! Whether someone is at the Mac, as reclaim needs to know it (B-1173). Two signals count and
//! nothing else does: a keyboard or mouse event in the last 30 seconds, or the frontmost app
//! holding a video wake lock. A display kept awake by anything else (a meeting, a presentation,
//! `caffeinate`) is not a person: Swift 0.3.11 counted it, and that took the headset off a phone
//! being used in a meeting. A reading that cannot be taken counts as nobody there.

/// Keyboard or mouse touched this recently: someone is at the Mac.
pub const USER_ACTIVE_WINDOW_S: f64 = 30.0;

/// Assertion names that mean "a video is playing". Sampled 2026-10-07 from `pmset -g assertions`:
/// Chrome (and Chromium builds) hold `NoDisplaySleepAssertion named: "Video Wake Lock"` from the
/// browser process while a video plays. Safari, QuickTime and IINA were not sampled; until they
/// are, they count as no video.
pub const VIDEO_ASSERTION_NAMES: [&str; 1] = ["Video Wake Lock"];

/// The two assertion types that keep the display on (`NoDisplaySleepAssertion` is the legacy
/// name `pmset` and `AssertType` still report for it).
pub const DISPLAY_ASSERTION_TYPES: [&str; 2] = ["PreventUserIdleDisplaySleep", "NoDisplaySleepAssertion"];

/// One display-sleep assertion: who holds it and what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayAssertion {
    pub pid: i32,
    pub process: String,
    pub name: String,
}

impl DisplayAssertion {
    pub fn is_video(&self) -> bool {
        VIDEO_ASSERTION_NAMES.contains(&self.name.as_str())
    }
}

/// The raw readings. `None` is a reading that could not be taken.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PresenceFacts {
    /// Seconds since the last keyboard, mouse or trackpad event (`HIDIdleTime`).
    pub input_idle: Option<f64>,
    /// Display-sleep assertions held right now. Read only when the keyboard is idle.
    pub display_assertions: Option<Vec<DisplayAssertion>>,
    /// The frontmost app's pid. Read only when a video assertion is held.
    pub front_pid: Option<i32>,
}

/// Why nobody counts as at the Mac. `key` leaves out the idle seconds so a reason that has not
/// changed is not logged again just because the seconds grew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Absence {
    pub text: String,
    pub key: String,
}

/// `Ok` when someone is at the Mac, otherwise the reason nobody is.
pub fn judge(facts: &PresenceFacts) -> Result<(), Absence> {
    let idle = match facts.input_idle {
        Some(s) if s < USER_ACTIVE_WINDOW_S => return Ok(()),
        Some(s) => format!("keyboard idle {s:.0} s"),
        None => "keyboard idle unknown".to_string(),
    };
    let video = match &facts.display_assertions {
        None => "display assertions unreadable".to_string(),
        Some(all) => {
            let videos: Vec<&DisplayAssertion> = all.iter().filter(|a| a.is_video()).collect();
            if videos.is_empty() {
                match all.first() {
                    Some(a) => format!("no foreground video; display held awake by {} \"{}\"", a.process, a.name),
                    None => "no foreground video".to_string(),
                }
            } else {
                match facts.front_pid {
                    None => "foreground app unknown".to_string(),
                    Some(front) if videos.iter().any(|a| a.pid == front) => return Ok(()),
                    Some(_) => format!("video in background app {}", videos[0].process),
                }
            }
        }
    };
    let text = format!("nobody at the Mac ({idle}, {video})");
    let key = format!("nobody at the Mac ({}, {video})", if facts.input_idle.is_some() { "idle" } else { "unknown" });
    Err(Absence { text, key })
}
