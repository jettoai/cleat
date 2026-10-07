//! Undo writes to the default output's volume made by the listed programs (Swift
//! `OutputVolumeHoldRule.swift`, 0.3.10). Which program wrote is not something CoreAudio says; the
//! ledger reads it from coreaudiod's log and hands this rule the facts.

use crate::model::{Action, AudioDevice};

/// Closer than this is the same volume.
pub const TOLERANCE: f32 = 0.0005;
/// Writer name the ledger records for Cleat's own writes.
pub const SELF_WRITER: &str = "cleat";

/// A listed writer's write still waiting to be judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Foreign {
    /// The value right before the listed writer touched the output.
    pub restore: f32,
    pub writer: String,
    /// Every value the write (and any echo of it) left on a channel.
    pub results: Vec<f32>,
    /// Somebody else wrote after it; it is theirs now.
    pub overruled: bool,
    pub prior_writer: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub current: Vec<f32>,
    pub held: Option<f32>,
    pub foreign: Option<Foreign>,
    pub last_writer: Option<String>,
    pub paused: bool,
}

/// A user change the listed write landed on top of before it was ever judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Prior {
    pub from: f32,
    pub to: f32,
    pub writer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    None,
    /// Remember silently: the first reading of a device, or our own write.
    Adopt(f32),
    /// The user's (or an unlisted program's) change: remember it.
    Kept { from: f32, to: f32, writer: String, note: Option<String> },
    /// A listed writer's change: write `restore` back.
    Revert { from: f32, restore: f32, writer: String, prior: Option<Prior> },
}

/// The config entry a writer's executable path matches: the executable's name, or an app the
/// executable lives inside (`Parallels Desktop` matches any `Parallels Desktop.app` on the path).
pub fn listed_entry(path: &str, entries: &[String]) -> Option<String> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    let executable = *components.last()?;
    entries
        .iter()
        .find(|e| !e.is_empty() && (executable == e.as_str() || components.contains(&format!("{e}.app").as_str())))
        .cloned()
}

pub fn level(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

pub fn decide(input: &Input) -> Verdict {
    let Some(level) = level(&input.current) else { return Verdict::None };
    let Some(held) = input.held else { return Verdict::Adopt(level) };

    if let Some(f) = &input.foreign {
        let still_theirs = input.current.iter().all(|v| f.results.iter().any(|r| (r - v).abs() <= TOLERANCE));
        if still_theirs && !f.overruled && (level - f.restore).abs() > TOLERANCE {
            if input.paused {
                return Verdict::Kept { from: held, to: level, writer: f.writer.clone(), note: Some("paused".into()) };
            }
            let prior = ((held - f.restore).abs() > TOLERANCE).then(|| Prior {
                from: held,
                to: f.restore,
                writer: f.prior_writer.clone().unwrap_or_else(|| "unknown".into()),
            });
            return Verdict::Revert { from: level, restore: f.restore, writer: f.writer.clone(), prior };
        }
    }

    if (level - held).abs() <= TOLERANCE {
        return Verdict::None;
    }
    if input.last_writer.as_deref() == Some(SELF_WRITER) {
        return Verdict::Adopt(level);
    }
    let writer = input
        .last_writer
        .clone()
        .or_else(|| input.foreign.as_ref().map(|f| f.writer.clone()))
        .unwrap_or_else(|| "unknown".into());
    Verdict::Kept { from: held, to: level, writer, note: None }
}

/// The write a `Revert` asks for; left and right get the same value.
pub fn actions(verdict: &Verdict, device: &AudioDevice) -> Vec<Action> {
    let Verdict::Revert { from, restore, writer, .. } = verdict else { return vec![] };
    let reason = format!(
        "{} {:.0}% -> {:.0}% (reverted {writer})",
        device.name,
        f64::from(*from) * 100.0,
        f64::from(*restore) * 100.0
    );
    vec![Action::SetOutputVolume(device.id, *restore, reason)]
}
