//! `rememberDisplays`' engine side (Swift `Engine+Displays.swift` and `Engine+RememberDisplays.swift`):
//! the debounced display pass, the memory file and the restore, word for word in the log.

use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

use super::run_loop::{Origin, Timer};
use super::{Engine, Mode};
use crate::display::DisplaySnapshot;
use crate::model::Action;
use crate::rules::remember_displays::{self as rule, Decision, Origins};
use crate::state::arrangements::{self, Arrangements, Loaded};

/// How long the display layout is left to settle after the last reconfiguration event.
pub const DISPLAY_DEBOUNCE_MS: u64 = 2000;
/// A display set macOS keeps rearranging is not worth fighting for ever.
pub const DISPLAY_PLACEMENT_LIMIT: u32 = 3;

/// What the engine remembers about displays between events.
#[derive(Default)]
pub(crate) struct DisplayState {
    /// The UUIDs online when `placements` was counted. A different set is a different arrangement
    /// problem and starts the count over.
    combination: BTreeSet<String>,
    /// Moves made since the set of displays or the config last changed. An arrangement seen right
    /// in between does not clear it: that is mostly the echo of our own write.
    placements: u32,
    gave_up: bool,
    /// Skip lines already written, so a standing reason is one line and not one per event.
    skips_logged: HashSet<String>,
    /// What is remembered, loaded from disk on first use. None until then.
    arrangements: Option<Arrangements>,
    /// When Cleat last wrote an arrangement, and when the set of displays last changed (a daemon's
    /// first pass counts), on the engine's clock. A difference soon after either is macOS settling,
    /// not the user.
    last_write_at: Option<Duration>,
    combination_changed_at: Option<Duration>,
    /// A reconfiguration event saw a set of UUIDs other than the one last handled. Debouncing can
    /// merge a short unplug and replug into one pass that sees the same set again; this still
    /// counts that as the set having changed.
    combination_event_pending: bool,
    /// The last restore failed or was given up on. Until the arrangement matches again, the set
    /// changes or the setting is edited, a difference is not the user's hand.
    restore_unresolved: bool,
}

impl DisplayState {
    fn reset_placements(&mut self) {
        self.placements = 0;
        self.gave_up = false;
        self.restore_unresolved = false;
    }
}

impl Engine {
    pub(super) fn start_displays(&mut self) {
        self.displays.start_observing();
        self.reconcile_displays(false);
    }

    /// Restarts the wait on every event; the pass runs once the events stop.
    pub(super) fn displays_changed(&mut self) {
        // An empty list is the system failing to answer, so it says nothing about the set.
        let displays = self.displays.snapshot();
        let uuids: BTreeSet<String> = displays.iter().map(|d| d.uuid.clone()).collect();
        if !uuids.is_empty() && uuids != self.display_state.combination {
            self.display_state.combination_event_pending = true;
        }
        let received = self.clock.wall();
        let origin = Origin { label: "displays", received, delay_ms: DISPLAY_DEBOUNCE_MS };
        let deadline = self.clock.mono() + Duration::from_millis(DISPLAY_DEBOUNCE_MS);
        self.scheduler.schedule(Timer::Displays, deadline, origin);
    }

    /// An edited `rememberDisplays` gets a fresh allowance: whatever stopped the last one was the
    /// old setting.
    pub(super) fn remember_displays_changed(&mut self) {
        self.display_state.reset_placements();
        self.reconcile_displays(false);
    }

    /// One display pass. `write_status` also runs a normal pass, which republishes status.json.
    pub fn reconcile_displays(&mut self, write_status: bool) {
        let displays = self.displays.snapshot();
        // An empty list is the system failing to answer, never a Mac with no screen.
        if displays.is_empty() {
            return;
        }
        let combination: BTreeSet<String> = displays.iter().map(|d| d.uuid.clone()).collect();
        // Also a set that came and went between two passes: the debounce merged the two events.
        let event_pending = std::mem::take(&mut self.display_state.combination_event_pending);
        let combination_changed = event_pending || combination != self.display_state.combination;
        if combination_changed {
            self.display_state.combination = combination;
            self.display_state.combination_changed_at = Some(self.clock.mono());
            self.display_state.reset_placements();
        }

        let mut skips = HashSet::new();
        self.remember_displays_pass(&displays, combination_changed, &mut skips);
        self.display_state.skips_logged = skips;
        if write_status {
            let origin = Origin { label: "displays", received: self.clock.wall(), delay_ms: DISPLAY_DEBOUNCE_MS };
            self.reconcile(false, Some(origin));
        }
    }

    fn remember_displays_pass(&mut self, displays: &[DisplaySnapshot], combination_changed: bool, skips: &mut HashSet<String>) {
        // Off means the memory file is never read or written, so a build with the feature unused
        // cannot touch a file it did not ask for.
        if !self.config.remember_displays {
            return;
        }
        self.load_arrangements_if_needed();

        let key = rule::key(displays);
        let st = &self.display_state;
        let anchor = [st.last_write_at, st.combination_changed_at].into_iter().flatten().max();
        let now = self.clock.mono();
        let remembered = st.arrangements.as_ref().and_then(|a| a.get(&key));
        let decision = rule::decide(
            displays,
            remembered,
            combination_changed,
            anchor.map(|a| now.saturating_sub(a)),
            st.restore_unresolved,
        );

        match decision {
            Decision::None => self.display_state.restore_unresolved = false,
            Decision::Skipped(line) => {
                skips.insert(line.clone());
                if !self.display_state.skips_logged.contains(&line) {
                    self.note(&line);
                }
            }
            Decision::Remember(origins, reason) => self.save_arrangement(&key, origins, displays, &reason),
            Decision::Restore(action) => {
                if self.display_state.placements >= DISPLAY_PLACEMENT_LIMIT {
                    self.display_state.restore_unresolved = true;
                    if !self.display_state.gave_up {
                        self.display_state.gave_up = true;
                        self.note(&format!("restoreDisplays gave up: macOS keeps rearranging {} displays", displays.len()));
                    }
                    return;
                }
                self.display_state.placements += 1;
                if !self.apply_display_origins(&action) {
                    self.display_state.restore_unresolved = true;
                }
            }
        }
    }

    /// Whether the write took (observe mode writes nothing and counts as taken).
    pub(super) fn apply_display_origins(&mut self, action: &Action) -> bool {
        let Action::SetDisplayOrigins(origins, reason) = action else { return true };
        if self.mode == Mode::Observe {
            self.note(&format!("{} {reason} [observe: not applied]", action.label()));
            return true;
        }
        let ok = self.displays.set_origins(origins);
        if ok {
            self.display_state.last_write_at = Some(self.clock.mono());
        }
        self.note(&format!("{} {reason}{}", action.label(), if ok { "" } else { " failed" }));
        ok
    }

    pub(super) fn remember_displays_summary(&self) -> String {
        if !self.config.remember_displays {
            return "off".into();
        }
        let count = self.display_state.arrangements.as_ref().map_or(0, |a| a.len());
        format!("on ({count} arrangement{} remembered)", if count == 1 { "" } else { "s" })
    }

    fn load_arrangements_if_needed(&mut self) {
        if self.display_state.arrangements.is_some() {
            return;
        }
        let (loaded, problem) = match arrangements::read(&self.arrangements_path) {
            Loaded::Empty => (Arrangements::new(), None),
            Loaded::Loaded(a) => (a, None),
            Loaded::Unreadable(reason) => (Arrangements::new(), Some(reason)),
        };
        self.display_state.arrangements = Some(loaded);
        if let Some(reason) = problem {
            let file = self.arrangements_path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned());
            self.note(&format!("rememberDisplays: could not read {file} ({reason}), starting empty"));
        }
    }

    fn save_arrangement(&mut self, key: &str, origins: Origins, displays: &[DisplaySnapshot], reason: &str) {
        let mut ordered: Vec<&DisplaySnapshot> = displays.iter().collect();
        ordered.sort_by_key(|d| d.id);
        let parts: Vec<String> = ordered
            .iter()
            .map(|d| {
                let [x, y] = origins.get(&d.uuid).copied().unwrap_or([0, 0]);
                format!("{} {} @{x},{y}", d.name, d.uuid.chars().take(8).collect::<String>())
            })
            .collect();
        let all = self.display_state.arrangements.get_or_insert_with(Arrangements::new);
        all.insert(key.to_string(), origins);
        // Observe mode shares the installed Cleat's file and must not write it.
        let suffix = if self.mode == Mode::Observe {
            " [observe: not written]"
        } else if arrangements::write(all, &self.arrangements_path) {
            ""
        } else {
            " failed"
        };
        self.note(&format!("rememberDisplays saved {} displays: {} ({reason}){suffix}", displays.len(), parts.join(" | ")));
    }
}
