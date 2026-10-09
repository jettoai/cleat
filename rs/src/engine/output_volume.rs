//! The output volume hold's engine side (Swift `Engine+OutputVolume.swift`, 0.3.10): the writer
//! source's life, the listener, the rule's pass and the status lines, word for word.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::run_loop::{Origin, Timer};
use super::{Engine, Mode};
use crate::model::{Action, DeviceSnapshot};
use crate::outvol::ledger::{Ledger, TUG_LIMIT};
use crate::outvol::writer_log::{VolumeWriterEvent, VolumeWriterSource};
use crate::rules::output_volume_hold::{actions, listed_entry, Verdict};
use crate::state::clock::local_seconds;

/// How long a volume change is left to settle; below the balance settle so a revert lands first.
pub const OUTPUT_VOLUME_SETTLE_MS: u64 = 300;
const RESTART_BACKOFF_S: [u64; 7] = [1, 2, 4, 8, 16, 32, 60];

pub type WriterSourceFactory = Box<dyn Fn() -> Box<dyn VolumeWriterSource>>;

#[derive(Default)]
pub struct OutputVolumeHold {
    pub(crate) ledger: Ledger,
    source: Option<Box<dyn VolumeWriterSource>>,
    source_problem: Option<String>,
    restart_attempt: usize,
    restart_pending: bool,
    output_uid: Option<String>,
}

/// The block `status.json` carries for the settings screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputVolumeStatus {
    pub state: String,
    /// The Swift-era sentence, kept word for word for older windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_revert: Option<String>,
    /// The same revert as data, so the window words it in its own language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_revert_detail: Option<RevertDetail>,
}

/// `from` and `to` are unrounded percents; the window rounds them as `lastRevert` does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertDetail {
    pub at: String,
    pub writer: String,
    pub from: f64,
    pub to: f64,
}

fn clock(t: SystemTime, seconds: bool) -> String {
    let s = local_seconds(t);
    s[11..if seconds { 19 } else { 16 }].to_string()
}

fn percent_line(name: &str, from: f32, to: f32) -> String {
    format!("outputVolume: {name} {:.0}% -> {:.0}%", f64::from(from) * 100.0, f64::from(to) * 100.0)
}

impl Engine {
    fn wall(&self) -> SystemTime {
        self.clock.wall()
    }

    /// Starts or stops the writer source to match the config. Observe mode starts nothing (D6).
    pub(crate) fn sync_output_volume_source(&mut self) {
        if self.config.hold_against_active().is_empty() || self.mode == Mode::Observe {
            let was_on = self.outvol.source.is_some() || self.outvol.restart_pending;
            if let Some(mut s) = self.outvol.source.take() {
                s.stop();
            }
            self.scheduler.cancel(Timer::SourceRestart);
            self.scheduler.cancel(Timer::StreakJudge);
            self.outvol.restart_pending = false;
            self.outvol.source_problem = None;
            self.outvol.restart_attempt = 0;
            if was_on {
                self.note("outputVolume: off");
            }
            return;
        }
        if self.outvol.source.is_none() && !self.outvol.restart_pending {
            self.start_output_volume_source();
        }
    }

    fn start_output_volume_source(&mut self) {
        let mut source = (self.writer_source)();
        if !source.start() {
            self.outvol.source_problem = Some("log stream could not start".into());
            self.note("outputVolume: writer log could not start, not reverting");
            self.schedule_source_restart();
            return;
        }
        self.outvol.source = Some(source);
        self.outvol.source_problem = None;
        self.note("outputVolume: reading writers from the system log");
    }

    pub(crate) fn volume_writer_exited(&mut self, status: i32) {
        self.outvol.source = None;
        if self.config.hold_against_active().is_empty() {
            return;
        }
        self.outvol.source_problem = Some(format!("log stream exited ({status})"));
        let delay = self.schedule_source_restart();
        self.note(&format!("outputVolume: writer log stopped (exit {status}), restarting in {delay}s"));
    }

    fn schedule_source_restart(&mut self) -> u64 {
        let delay = RESTART_BACKOFF_S[self.outvol.restart_attempt.min(RESTART_BACKOFF_S.len() - 1)];
        self.outvol.restart_attempt += 1;
        self.outvol.restart_pending = true;
        let origin = Origin { label: "outputVolume", received: self.wall(), delay_ms: delay * 1000 };
        let deadline = self.clock.mono() + Duration::from_secs(delay);
        self.scheduler.schedule(Timer::SourceRestart, deadline, origin);
        delay
    }

    pub(crate) fn source_restart_due(&mut self) {
        self.outvol.restart_pending = false;
        if !self.config.hold_against_active().is_empty() {
            self.start_output_volume_source();
        }
    }

    pub(crate) fn volume_writer_event(&mut self, event: VolumeWriterEvent) {
        match event {
            VolumeWriterEvent::Unrecognised(message) => {
                if self.outvol.ledger.note_unrecognised(&message) {
                    let head: String = message.chars().take(120).collect();
                    self.note(&format!("outputVolume: unrecognised writer line, not reverting: {head}"));
                }
            }
            VolumeWriterEvent::Write(mut write) => {
                self.outvol.restart_attempt = 0;
                let uid = self.outvol.output_uid.clone().or_else(|| self.current_output_uid());
                // Named from here on by the entry it matched, so the log says what the config says.
                let entry = listed_entry(&write.writer, self.config.hold_against_active());
                write.writer = entry.clone().unwrap_or_else(|| write.writer.rsplit('/').next().unwrap_or("").to_string());
                let now = self.wall();
                if let Some(deadline) =
                    self.outvol.ledger.record(write, entry.is_some(), std::process::id() as i32, uid.as_deref(), now)
                {
                    let wait = deadline.duration_since(now).unwrap_or_default();
                    let origin = Origin { label: "outputVolume", received: now, delay_ms: wait.as_millis() as u64 };
                    self.scheduler.schedule(Timer::StreakJudge, self.clock.mono() + wait, origin);
                }
            }
        }
    }

    fn current_output_uid(&self) -> Option<String> {
        let snap = self.system.snapshot(&self.config);
        snap.output_device().map(|d| d.uid.clone())
    }

    /// The volume listener: record the reading and judge once it settles. A reading from a
    /// device that is no longer the output is dropped.
    pub fn output_volume_changed(&mut self, device: u32) {
        let snap = self.system.snapshot(&self.config);
        if snap.default_output != Some(device) {
            return;
        }
        let Some(uid) = snap.device(device).map(|d| d.uid.clone()) else { return };
        let now = self.wall();
        self.outvol.ledger.observe(&snap.output_volumes, &uid, now);
        self.outvol.ledger.volume_changed(now);
        self.schedule_reconcile(OUTPUT_VOLUME_SETTLE_MS);
    }

    pub(crate) fn output_volume_actions(&mut self, snap: &DeviceSnapshot) -> Vec<Action> {
        if self.config.hold_against_active().is_empty() {
            return vec![];
        }
        let Some(device) = snap.output_device().cloned() else { return vec![] };
        self.outvol.output_uid = Some(device.uid.clone());
        let now = self.wall();
        self.outvol.ledger.observe(&snap.output_volumes, &device.uid, now);
        let source_down = self.outvol.source_problem.is_some();
        let judgement = self.outvol.ledger.judge(&device, &snap.output_volumes, now, source_down);
        match &judgement.verdict {
            Verdict::Kept { from, to, writer, note } => {
                let remark = note.as_ref().map_or(String::new(), |n| format!(" ({n})"));
                self.note(&format!("{} writer={writer} kept{remark}", percent_line(&device.name, *from, *to)));
            }
            Verdict::Revert { from, restore, writer, prior } => {
                if let Some(p) = prior {
                    self.note(&format!("{} writer={} kept", percent_line(&device.name, p.from, p.to), p.writer));
                }
                self.note(&format!("{} writer={writer} reverted", percent_line(&device.name, *restore, *from)));
            }
            Verdict::None | Verdict::Adopt(_) => {}
        }
        if judgement.started_pause {
            if let Some(until) = self.outvol.ledger.paused_until {
                let writer = self.outvol.ledger.paused_by.clone().unwrap_or_else(|| "unknown".into());
                self.note(&format!(
                    "outputVolume: paused until {}, {writer} changed the volume back {} times in a minute; turning off Parallels' \"Sync volume with Mac\" stops it",
                    clock(until, false),
                    TUG_LIMIT + 1
                ));
            }
        }
        actions(&judgement.verdict, &device)
    }

    fn output_volume_state(&self) -> String {
        let list = self.config.hold_against_active();
        if list.is_empty() {
            return "off".into();
        }
        if let Some(reason) = self.outvol.source_problem.clone().or_else(|| self.outvol.ledger.blind()) {
            return format!("paused: cannot read system log writer ({reason})");
        }
        if let Some(until) = self.outvol.ledger.paused_until {
            let writer = self.outvol.ledger.paused_by.clone().unwrap_or_else(|| "unknown".into());
            return format!("paused: tug-of-war with {writer}, resumes {}", clock(until, false));
        }
        format!("on (holding against {})", list.join(", "))
    }

    pub(crate) fn output_volume_summary(&self) -> String {
        let mut s = self.output_volume_state();
        if let Some(last) = &self.outvol.ledger.last_revert {
            s += &format!(
                "; last revert {} {:.0}% -> {:.0}% ({})",
                clock(last.at, true),
                f64::from(last.from) * 100.0,
                f64::from(last.to) * 100.0,
                last.writer
            );
        }
        s
    }

    pub(crate) fn output_volume_status(&self) -> OutputVolumeStatus {
        let last_revert_detail = self.outvol.ledger.last_revert.as_ref().map(|last| RevertDetail {
            at: clock(last.at, false),
            writer: last.writer.clone(),
            from: f64::from(last.from) * 100.0,
            to: f64::from(last.to) * 100.0,
        });
        let last_revert = last_revert_detail
            .as_ref()
            .map(|d| format!("{} 拉回 {} 改的音量 {:.0}% → {:.0}%", d.at, d.writer, d.from, d.to));
        OutputVolumeStatus { state: self.output_volume_state(), last_revert, last_revert_detail }
    }
}
