//! The output volume hold's memory (Swift `OutputVolumeLedger.swift`, 0.3.10): what each device
//! is held at, the listed writer's open streak, the tug-of-war pause, and whether the writer log
//! can still be trusted. Constants copied as they are.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use super::writer_log::VolumeWrite;
use crate::model::AudioDevice;
use crate::rules::output_volume_hold::{decide, Foreign, Input, Verdict, SELF_WRITER, TOLERANCE};

pub const JUDGE_DELAY: Duration = Duration::from_millis(300);
pub const ECHO_GRACE: Duration = Duration::from_millis(100);
pub const ECHO_WRITER: &str = "ControlCenter";
pub const STALE_AFTER: Duration = Duration::from_secs(1);
pub const TUG_WINDOW: Duration = Duration::from_secs(60);
pub const TUG_LIMIT: usize = 3;
pub const TUG_PAUSE: Duration = Duration::from_secs(600);
pub const BLIND_AFTER: usize = 10;
pub const ATTRIBUTION_WINDOW: Duration = Duration::from_millis(150);
pub const CHANGE_MEMORY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
struct Streak {
    uid: String,
    started_at: SystemTime,
    last_foreign_at: SystemTime,
    listed: Vec<VolumeWrite>,
    echoes: Vec<VolumeWrite>,
    overruled: bool,
    prior_writer: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct OutputChange {
    at: SystemTime,
    from: f32,
    to: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Revert {
    pub at: SystemTime,
    pub from: f32,
    pub to: f32,
    pub writer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Judgement {
    pub verdict: Verdict,
    pub started_pause: bool,
}

/// |a - b| in seconds, whichever is later.
fn gap(a: SystemTime, b: SystemTime) -> Duration {
    a.duration_since(b).or_else(|_| b.duration_since(a)).unwrap_or_default()
}

/// a - b, zero when b is later.
fn since(a: SystemTime, b: SystemTime) -> Duration {
    a.duration_since(b).unwrap_or_default()
}

#[derive(Debug, Default)]
pub struct Ledger {
    held: HashMap<String, f32>,
    streak: Option<Streak>,
    last_writer: Option<String>,
    last_change_at: Option<SystemTime>,
    revert_times: Vec<SystemTime>,
    pub paused_until: Option<SystemTime>,
    pub paused_by: Option<String>,
    pub last_revert: Option<Revert>,
    unattributed_in_a_row: usize,
    unrecognised: Option<String>,
    observed: Vec<f32>,
    observed_uid: Option<String>,
    output_changes: Vec<OutputChange>,
}

impl Ledger {
    pub fn held(&self, uid: &str) -> Option<f32> {
        self.held.get(uid).copied()
    }

    pub fn revert_count(&self) -> usize {
        self.revert_times.len()
    }

    pub fn has_streak(&self) -> bool {
        self.streak.is_some()
    }

    /// When the open streak and the last change each fall due for judgement.
    pub fn due(&self) -> (Option<SystemTime>, Option<SystemTime>) {
        (self.streak.as_ref().map(|s| s.started_at + JUDGE_DELAY), self.last_change_at.map(|t| t + JUDGE_DELAY))
    }

    /// Records one parsed line; returns when to judge if it opened a listed writer's streak.
    pub fn record(&mut self, write: VolumeWrite, listed: bool, own_pid: i32, output_uid: Option<&str>, now: SystemTime) -> Option<SystemTime> {
        self.unattributed_in_a_row = 0;
        self.unrecognised = None;
        if since(now, write.at) > STALE_AFTER || !write.changes_value() {
            return None;
        }
        if write.pid == own_pid {
            self.last_writer = Some(SELF_WRITER.into());
            return None;
        }
        if listed {
            let uid = output_uid?;
            if let Some(open) = &mut self.streak {
                open.last_foreign_at = write.at;
                open.listed.push(write);
                return None;
            }
            let at = write.at;
            self.streak = Some(Streak {
                uid: uid.into(),
                started_at: at,
                last_foreign_at: at,
                listed: vec![write],
                echoes: vec![],
                overruled: false,
                prior_writer: self.last_writer.clone(),
            });
            return Some(at + JUDGE_DELAY);
        }
        if let Some(open) = &mut self.streak {
            let is_echo = write.writer == ECHO_WRITER
                && since(write.at, open.last_foreign_at) <= ECHO_GRACE
                && open.listed.iter().chain(&open.echoes).any(|w| (w.to - write.from).abs() <= TOLERANCE);
            if is_echo {
                open.echoes.push(write);
                return None;
            }
            open.overruled = true;
        }
        self.last_writer = Some(write.writer);
        None
    }

    /// True the first time an unrecognised line arrives since the last one that parsed.
    pub fn note_unrecognised(&mut self, message: &str) -> bool {
        let first = self.unrecognised.is_none();
        self.unrecognised = Some(message.into());
        first
    }

    pub fn volume_changed(&mut self, at: SystemTime) {
        self.last_change_at = Some(at);
    }

    /// A reading of the output's channels; changes are kept briefly to attribute writer lines.
    pub fn observe(&mut self, values: &[f32], uid: &str, at: SystemTime) {
        if self.observed_uid.as_deref() != Some(uid) {
            self.observed_uid = Some(uid.into());
            self.output_changes.clear();
        } else if values.len() == self.observed.len() {
            for (old, new) in self.observed.iter().zip(values) {
                if (old - new).abs() > TOLERANCE {
                    self.output_changes.push(OutputChange { at, from: *old, to: *new });
                }
            }
        }
        self.observed = values.to_vec();
        self.output_changes.retain(|c| since(at, c.at) <= CHANGE_MEMORY);
    }

    fn moved_output(&self, w: &VolumeWrite) -> bool {
        self.output_changes.iter().any(|c| {
            (c.from - w.from).abs() <= TOLERANCE && (c.to - w.to).abs() <= TOLERANCE && gap(c.at, w.at) <= ATTRIBUTION_WINDOW
        })
    }

    fn foreign(&self, open: &Streak) -> Option<Foreign> {
        let output: Vec<&VolumeWrite> = open.listed.iter().filter(|w| self.moved_output(w)).collect();
        let first = output.iter().min_by_key(|w| w.at)?;
        let results = output.iter().copied().chain(open.echoes.iter().filter(|w| self.moved_output(w))).map(|w| w.to).collect();
        Some(Foreign {
            restore: first.from,
            writer: first.writer.clone(),
            results,
            overruled: open.overruled,
            prior_writer: open.prior_writer.clone(),
        })
    }

    pub fn is_settling(&self, now: SystemTime) -> bool {
        let Some(start) = self.streak.as_ref().map(|s| s.started_at).or(self.last_change_at) else { return false };
        now < start + JUDGE_DELAY
    }

    pub fn judge(&mut self, device: &AudioDevice, current: &[f32], now: SystemTime, source_down: bool) -> Judgement {
        let idle = Judgement { verdict: Verdict::None, started_pause: false };
        if current.is_empty() || self.is_settling(now) {
            return idle;
        }
        if self.streak.as_ref().is_some_and(|s| s.uid != device.uid) {
            self.streak = None;
        }
        if self.paused_until.is_some_and(|until| now >= until) {
            self.paused_until = None;
            self.paused_by = None;
        }
        self.revert_times.retain(|t| since(now, *t) <= TUG_WINDOW);

        let foreign = self.streak.as_ref().and_then(|s| self.foreign(s));
        let mut verdict = decide(&Input {
            current: current.to_vec(),
            held: self.held(&device.uid),
            foreign,
            last_writer: self.last_writer.clone(),
            paused: self.paused_until.is_some() || source_down || self.blind().is_some(),
        });
        let mut started_pause = false;
        if let Verdict::Revert { from, writer, .. } = &verdict {
            if self.revert_times.len() >= TUG_LIMIT {
                self.paused_until = Some(now + TUG_PAUSE);
                self.paused_by = Some(writer.clone());
                started_pause = true;
                verdict = Verdict::Kept {
                    from: self.held(&device.uid).unwrap_or(*from),
                    to: *from,
                    writer: writer.clone(),
                    note: Some("paused".into()),
                };
            }
        }
        match &verdict {
            Verdict::Adopt(v) | Verdict::Kept { to: v, .. } => {
                self.held.insert(device.uid.clone(), *v);
            }
            Verdict::Revert { from, restore, writer, .. } => {
                self.held.insert(device.uid.clone(), *restore);
                self.revert_times.push(now);
                self.last_revert = Some(Revert { at: now, from: *from, to: *restore, writer: writer.clone() });
            }
            Verdict::None => {}
        }
        if device.is_bluetooth() && matches!(&verdict, Verdict::Kept { writer, .. } if writer == "unknown") {
            self.unattributed_in_a_row += 1;
        }
        self.streak = None;
        self.last_writer = None;
        Judgement { verdict, started_pause }
    }

    /// Why the writer log cannot be trusted right now, if it cannot.
    pub fn blind(&self) -> Option<String> {
        if self.unrecognised.is_some() {
            return Some("unrecognised writer line".into());
        }
        (self.unattributed_in_a_row >= BLIND_AFTER)
            .then(|| format!("{} Bluetooth volume changes without a writer line", self.unattributed_in_a_row))
    }
}
