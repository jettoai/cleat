//! The pure formatting behind the words in `words.rs` (Swift `SettingsView.swift:22-48`,
//! `LevelRows.swift`, `DaemonVitals.swift:70-168`, `SettingsIcon.swift`). Every function that
//! returns words takes the `Lang` to speak, so it stays pure.

use objc2_core_audio::{
    kAudioDeviceTransportTypeAirPlay, kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioDeviceTransportTypeBuiltIn, kAudioDeviceTransportTypeDisplayPort, kAudioDeviceTransportTypeHDMI,
};

use super::draft::{DeviceRow, HeadsetBox, ReclaimHint, Side};
use super::lang::Lang;
use super::sources::LastRevert;
use super::vitals::{DaemonVitals, VitalsState};
use super::words::W;
use crate::state::reaction_clock::DaemonPerformance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Output,
    Input,
    Headphones,
    General,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Output, Page::Input, Page::Headphones, Page::General];

    pub fn title(self, l: Lang) -> &'static str {
        match self {
            Page::Output => W::PageOutput,
            Page::Input => W::PageInput,
            Page::Headphones => W::PageHeadphones,
            Page::General => W::PageGeneral,
        }
        .get(l)
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Page::Output => "speaker.wave.2",
            Page::Input => "mic",
            Page::Headphones => "beats.headphones",
            Page::General => "gearshape",
        }
    }

    pub fn subtitle(self, l: Lang) -> &'static str {
        match self {
            Page::Output => W::SubOutput,
            Page::Input => W::SubInput,
            Page::Headphones => W::SubHeadphones,
            Page::General => W::SubGeneral,
        }
        .get(l)
    }
}

/// Puts `args` into the template's `{}` in order. A value containing "{}" is not scanned again.
pub fn fill(template: &str, args: &[&str]) -> String {
    let mut parts = template.split("{}");
    let mut out = parts.next().unwrap_or_default().to_string();
    for (i, p) in parts.enumerate() {
        out.push_str(args.get(i).copied().unwrap_or(""));
        out.push_str(p);
    }
    out
}

/// One of two words by side.
fn by_side(side: Side, output: W, input: W) -> W {
    match side {
        Side::Output => output,
        Side::Input => input,
    }
}

pub fn device_list_footer(side: Side, l: Lang) -> &'static str {
    by_side(side, W::FooterOutput, W::FooterInput).get(l)
}

/// The grey words after a headset's name: why it is greyed out, or that it is not connected.
pub fn headset_note(shown: HeadsetBox, connected: bool, l: Lang) -> Option<&'static str> {
    match shown {
        HeadsetBox::BlockedTicked => Some(W::HeadsetBlockedNote.get(l)),
        HeadsetBox::BlockedOff => Some(W::HeadsetBlockedOffNote.get(l)),
        HeadsetBox::Plain(_) => (!connected).then(|| W::NotConnected.get(l)),
    }
}

/// The line under the headphones list while reclaim is on but will do nothing (B-1287).
pub fn reclaim_hint_text(hint: ReclaimHint, l: Lang) -> &'static str {
    match hint {
        ReclaimHint::NoHeadsets => W::NoHeadsets,
        ReclaimHint::AllHeadsetsBlocked => W::AllHeadsetsBlockedHint,
        ReclaimHint::NoneTicked => W::NoneTicked,
        ReclaimHint::AllTickedBlocked => W::AllHeadsetsBlocked,
    }
    .get(l)
}

/// The page header's orange lines for one side (B-1287): left on a "not used" device because
/// nothing else is usable, and a "not used" device the eviction cooldown leaves in place. Each says
/// what Albert can do. The resume conditions follow `engine::eviction`'s log line.
pub fn stuck_header(side: Side, stuck: Option<&str>, paused: Option<&str>, l: Lang) -> Vec<String> {
    let s = by_side(side, W::StuckOutput, W::StuckInput).get(l);
    let p = by_side(side, W::PausedOutput, W::PausedInput).get(l);
    stuck.map(|n| fill(s, &[n])).into_iter().chain(paused.map(|n| fill(p, &[n]))).collect()
}

/// 0.5 is "置中"; otherwise the side and |v - 0.5| x 200 percent.
pub fn balance_describe(value: f64, l: Lang) -> String {
    let percent = ((value - 0.5).abs() * 200.0).round() as i64;
    if percent == 0 {
        return W::Centre.get(l).into();
    }
    fill(if value < 0.5 { W::LeftPct } else { W::RightPct }.get(l), &[&percent.to_string()])
}

/// "現在：AirPods Max 62%", or why there is no reading.
pub fn now_text(device: Option<&str>, reading: Option<&str>, side: Side, l: Lang) -> String {
    match (device, reading) {
        (None, _) => by_side(side, W::NowNoneOutput, W::NowNoneInput).get(l).into(),
        (Some(d), None) => fill(W::NowNoReading.get(l), &[d]),
        (Some(d), Some(r)) => fill(W::NowReading.get(l), &[d, r]),
    }
}

/// The output volume hold's last revert: the daemon's structured detail in either language, or the
/// sentence an older daemon wrote (Chinese; in English only its clock survives).
pub fn revert_line(r: Option<&LastRevert>, l: Lang) -> String {
    match r {
        None => W::NoUndoYet.get(l).into(),
        Some(LastRevert::Detail(d)) => {
            let (from, to) = (format!("{:.0}", d.from), format!("{:.0}", d.to));
            fill(W::RevertLine.get(l), &[&d.at, &d.writer, &from, &to])
        }
        Some(LastRevert::Legacy(s)) => match l {
            Lang::ZhHant => fill(W::RevertLegacy.zh(), &[s]),
            Lang::En => match s.get(..5).filter(|c| c.as_bytes().get(2) == Some(&b':') && c.chars().filter(char::is_ascii_digit).count() == 4) {
                Some(clock) => fill(W::RevertLegacy.en(), &[clock]),
                None => W::NoUndoYet.en().into(),
            },
        },
    }
}

pub fn percent_text(v: f64) -> String {
    format!("{}%", v.round() as i64)
}

pub fn cpu_text(percent: f64) -> String {
    if percent < 0.1 {
        format!("{percent:.2}%")
    } else if percent < 10.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.0}%")
    }
}

pub fn memory_text(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

pub fn vitals_headline(state: VitalsState, l: Lang) -> &'static str {
    match state {
        VitalsState::Running => W::Running,
        VitalsState::NotRunning => W::NotRunning,
        VitalsState::Unreadable => W::VitalsUnreadable,
    }
    .get(l)
}

pub fn vitals_cpu(v: &DaemonVitals, l: Lang) -> String {
    match v.cpu_percent {
        Some(c) => cpu_text(c),
        None if v.state == VitalsState::Running && v.cpu_measuring => W::Measuring.get(l).into(),
        None => "—".into(),
    }
}

pub fn vitals_cpu_note(v: &DaemonVitals, l: Lang) -> String {
    v.cpu_window_seconds.map_or_else(|| W::CpuNote.get(l).into(), |s| fill(W::CpuWindow.get(l), &[&s.to_string()]))
}

pub fn vitals_memory(v: &DaemonVitals) -> String {
    v.footprint_bytes.map_or_else(|| "—".into(), memory_text)
}

/// Swift `PerformanceFormat.millis`.
pub fn millis_text(ms: f64) -> String {
    if ms < 10.0 {
        format!("{ms:.1} ms")
    } else if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

/// Only a performance block with at least one write-back has a figure to show.
fn reaction(v: &DaemonVitals) -> Option<&DaemonPerformance> {
    v.performance.as_ref().filter(|p| v.state == VitalsState::Running && p.samples > 0)
}

pub fn reaction_value(v: &DaemonVitals) -> String {
    reaction(v).and_then(|p| p.median_reaction_ms).map_or_else(|| "—".into(), millis_text)
}

pub fn reaction_note(v: &DaemonVitals, l: Lang) -> String {
    reaction(v).map_or_else(|| W::ReactionNone.get(l).into(), |p| fill(W::ReactionMedian.get(l), &[&p.samples.to_string()]))
}

pub fn reaction_help(v: &DaemonVitals, l: Lang) -> String {
    if v.state != VitalsState::Running {
        return W::ReactionNotRunning.get(l).into();
    }
    let Some(p) = &v.performance else { return W::ReactionOldDaemon.get(l).into() };
    let (Some(total), Some(work)) = (p.last_reaction_ms, p.last_work_ms) else { return W::ReactionNone.get(l).into() };
    if p.samples == 0 {
        return W::ReactionNone.get(l).into();
    }
    fill(W::ReactionHelp.get(l), &[&millis_text(total), &millis_text(work)])
}

/// Which SF Symbol stands for a device, from its name and bus. The caller checks the symbol exists.
pub fn device_symbol(name: &str, transport: Option<u32>, side: Side) -> &'static str {
    let lower = name.to_lowercase();
    let is = |t: u32| transport == Some(t);
    let any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if is(kAudioDeviceTransportTypeBluetooth)
        || is(kAudioDeviceTransportTypeBluetoothLE)
        || any(&["airpods", "headphone", "耳機", "buds"])
    {
        "beats.headphones"
    } else if is(kAudioDeviceTransportTypeHDMI) || is(kAudioDeviceTransportTypeDisplayPort) || any(&["display", "顯示器"]) {
        "display"
    } else if is(kAudioDeviceTransportTypeAirPlay) {
        "hifispeaker"
    } else if side == Side::Output && is(kAudioDeviceTransportTypeBuiltIn) {
        if lower.contains("macbook") {
            "laptopcomputer"
        } else if lower.contains("studio") {
            "macstudio"
        } else {
            "display"
        }
    } else if any(&["brio", "webcam", "camera"]) {
        "web.camera"
    } else if side == Side::Input {
        "mic"
    } else {
        "hifispeaker"
    }
}

/// The attribution line under the settings sidebar.
pub fn byline(version: &str) -> String {
    format!("Cleat {version} · {BYLINE_CREDIT}")
}

/// The attribution, also the About panel's credits.
pub const BYLINE_CREDIT: &str = "by Jetto";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Accent,
    Warning,
    Plain,
}

/// The row is the device the side is on right now.
pub fn in_use(r: &DeviceRow, current: Option<&str>) -> bool {
    r.is_connected && current.is_some_and(|c| crate::model::device_name::matches(&r.display_name, c, ""))
}

/// Why the daemon left a side on this row, from the running daemon's `stuck` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowReason {
    Stuck,
    Paused,
    None,
}

/// A device row's badges. The device in use reads "使用中" even when it is "not used" (B-1287: the
/// side is left on it), so the row, the header and the level line name the same device; an orange
/// badge says why when the daemon reports it, else "已排除" stays grey. Any other "not used" device
/// reads "已排除".
pub fn device_row_tags(r: &DeviceRow, current: Option<&str>, reason: RowReason, l: Lang) -> Vec<(&'static str, Tone)> {
    let why = match reason {
        RowReason::Stuck => (W::TagStuck.get(l), Tone::Warning),
        RowReason::Paused => (W::TagPaused.get(l), Tone::Warning),
        RowReason::None => (W::Excluded.get(l), Tone::Plain),
    };
    let in_use_tag = (W::InUse.get(l), Tone::Accent);
    match (in_use(r, current), r.is_blocked()) {
        (true, true) => vec![in_use_tag, why],
        (true, false) => vec![in_use_tag],
        (false, true) => vec![(W::Excluded.get(l), Tone::Plain)],
        (false, false) => vec![],
    }
}

/// Only a "not used" device the side is not on is greyed out; the one in use stays readable.
pub fn row_dimmed(r: &DeviceRow, current: Option<&str>) -> bool {
    r.is_blocked() && !in_use(r, current)
}
