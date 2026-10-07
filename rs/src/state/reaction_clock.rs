//! How long the engine took to put things back after a CoreAudio event (Swift `ReactionClock.swift`):
//! from the event reaching the engine to the last successful write of the pass it caused. Times are
//! monotonic nanoseconds passed in by the caller, so this is a pure value and tests need no clock.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::clock::iso8601_utc;

/// status.json's `performance` block (Swift `DaemonPerformance`). Optional fields are left out
/// when there is nothing to say.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonPerformance {
    pub samples: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reaction_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_work_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub median_reaction_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reaction_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reaction {
    /// Event in to write done, including the settle beats the engine waits on purpose.
    pub total_nanos: u64,
    /// The pass alone: reconcile start to write done.
    pub work_nanos: u64,
    pub at: SystemTime,
}

#[derive(Debug, Clone, Default)]
pub struct ReactionClock {
    recent: Vec<Reaction>,
    event_at: Option<u64>,
    /// When the last beat the pending events asked for is due. A pass that ends after it and
    /// wrote nothing gives the event up: whatever writes later is answering something else.
    settles_by: u64,
    pass_start: Option<u64>,
    recorded_this_pass: bool,
}

impl ReactionClock {
    pub const CAPACITY: usize = 20;

    pub fn recent(&self) -> &[Reaction] {
        &self.recent
    }

    /// The first event of a burst is the start; later ones in the same burst do not move it.
    /// `due` is when the last beat this event scheduled is due (`now` for a zero-delay one).
    pub fn event_arrived(&mut self, now: u64, due: u64) {
        self.event_at.get_or_insert(now);
        self.settles_by = self.settles_by.max(due);
    }

    pub fn pass_began(&mut self, now: u64) {
        self.pass_start = Some(now);
        self.recorded_this_pass = false;
    }

    /// A write that returned noErr. Ignored outside a scheduled pass or without an event.
    pub fn wrote(&mut self, now: u64, date: SystemTime) {
        let (Some(event_at), Some(pass_start)) = (self.event_at, self.pass_start) else { return };
        if now < pass_start || pass_start < event_at {
            return;
        }
        let reaction = Reaction { total_nanos: now - event_at, work_nanos: now - pass_start, at: date };
        match self.recent.last_mut() {
            Some(last) if self.recorded_this_pass => *last = reaction,
            _ => {
                self.recent.push(reaction);
                if self.recent.len() > Self::CAPACITY {
                    let excess = self.recent.len() - Self::CAPACITY;
                    self.recent.drain(..excess);
                }
                self.recorded_this_pass = true;
            }
        }
    }

    /// A pass that wrote consumes its event; so does a pass at or past the event's last beat.
    /// Not "no beat is pending": unrelated beats (reclaim retries) can stay pending for minutes,
    /// and the echo of Cleat's own write would then start the next reaction.
    pub fn pass_ended(&mut self, now: u64) {
        if self.recorded_this_pass || now >= self.settles_by {
            self.event_at = None;
            self.settles_by = 0;
        }
        self.pass_start = None;
        self.recorded_this_pass = false;
    }

    pub fn summary(&self) -> DaemonPerformance {
        let Some(last) = self.recent.last() else { return DaemonPerformance::default() };
        let mut totals: Vec<u64> = self.recent.iter().map(|r| r.total_nanos).collect();
        totals.sort_unstable();
        let middle = totals.len() / 2;
        let median = if totals.len() % 2 == 1 {
            totals[middle] as f64
        } else {
            (totals[middle - 1] as f64 + totals[middle] as f64) / 2.0
        };
        DaemonPerformance {
            samples: self.recent.len(),
            last_reaction_ms: Some(last.total_nanos as f64 / 1e6),
            last_work_ms: Some(last.work_nanos as f64 / 1e6),
            median_reaction_ms: Some(median / 1e6),
            last_reaction_at: Some(iso8601_utc(last.at)),
        }
    }
}
