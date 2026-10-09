//! Every word the settings window shows, and the pure formatting behind them (Swift
//! `SettingsView.swift:22-48`, `LevelRows.swift`, `DaemonVitals.swift:70-168`, `SettingsIcon.swift`).

use objc2_core_audio::{
    kAudioDeviceTransportTypeAirPlay, kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioDeviceTransportTypeBuiltIn, kAudioDeviceTransportTypeDisplayPort, kAudioDeviceTransportTypeHDMI,
};

use super::draft::{DeviceRow, HeadsetBox, ReclaimHint, Side};
use super::vitals::{DaemonVitals, VitalsState};
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

    pub fn title(self) -> &'static str {
        match self {
            Page::Output => "輸出",
            Page::Input => "輸入",
            Page::Headphones => "耳機",
            Page::General => "一般",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Page::Output => "speaker.wave.2",
            Page::Input => "mic",
            Page::Headphones => "beats.headphones",
            Page::General => "gearshape",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Output => "Cleat 讓聲音一直從你排第一的裝置出來",
            Page::Input => "Cleat 讓你排第一的麥克風一直是預設輸入，音量停在你設定的值",
            Page::Headphones => "藍牙耳機被手機或 iPad 拿走時，Cleat 把它要回來",
            Page::General => "Cleat 什麼時候執行",
        }
    }
}

pub fn noun(side: Side) -> &'static str {
    match side {
        Side::Input => "輸入",
        Side::Output => "輸出",
    }
}

pub fn device_list_footer(side: Side) -> &'static str {
    match side {
        Side::Input => "清單中第一個已連線的麥克風會成為預設輸入。勾「不使用」的裝置，Cleat 永遠不會切過去。",
        Side::Output => "有聲音要播時，Cleat 會切到清單中第一個已連線的裝置。勾「不使用」的裝置，Cleat 永遠不會切過去。",
    }
}

pub const OUTPUT_LEVELS_FOOTER: &str = "輸出音量：名單上的程式改了音量，Cleat 會拉回原本的值；你自己調的不會被拉回。左右平衡：打開「固定」後，被別的 app 或藍牙重連改掉時 Cleat 會改回來。";
pub const VOLUMES_FOOTER: &str = "單獨設定的麥克風優先於預設音量。";
pub const LAUNCH_AT_LOGIN_NOTE: &str = "開著時，開機登入後就會啟動 Cleat，當掉也會自動重開。關掉後 Cleat 會結束，之後要用時從「應用程式」資料夾打開。";
pub const OTHERS_NOTE: &str = "系統沒說是什麼的藍牙裝置。喇叭或耳機不在上面時，到這裡勾；手機、電腦不用勾。";
pub const HEADSET_BLOCKED_NOTE: &str = "輸出設為不使用，不會拉回";
pub const HEADSET_BLOCKED_OFF_NOTE: &str = "輸出設為不使用";
/// The orange line under the name of the device a side is left on (B-1287); the page header says why.
pub const STUCK_ROW_NOTE: &str = "暫時還在用這台";
pub const ALL_HEADSETS_BLOCKED: &str = "勾選的耳機都設為不使用，不會有動作";

/// The grey words after a headset's name: why it is greyed out, or that it is not connected.
pub fn headset_note(shown: HeadsetBox, connected: bool) -> Option<&'static str> {
    match shown {
        HeadsetBox::BlockedTicked => Some(HEADSET_BLOCKED_NOTE),
        HeadsetBox::BlockedOff => Some(HEADSET_BLOCKED_OFF_NOTE),
        HeadsetBox::Plain(_) => (!connected).then_some("未連線"),
    }
}

/// The line under the headphones list while reclaim is on but will do nothing (B-1287).
pub fn reclaim_hint_text(hint: ReclaimHint) -> &'static str {
    match hint {
        ReclaimHint::NoHeadsets => "還沒有配對過的耳機，不會有動作",
        ReclaimHint::AllHeadsetsBlocked => "所有耳機都設為不使用，不會有動作。到「輸出」頁取消「不使用」即可。",
        ReclaimHint::NoneTicked => "請至少勾選一副耳機，否則不會有動作",
        ReclaimHint::AllTickedBlocked => ALL_HEADSETS_BLOCKED,
    }
}

/// The page header's orange lines for one side (B-1287): left on a "not used" device because
/// nothing else is usable, and a "not used" device the eviction cooldown leaves in place. Each says
/// what Albert can do. The resume conditions follow `engine::eviction`'s log line.
pub fn stuck_header(side: Side, stuck: Option<&str>, paused: Option<&str>) -> Vec<String> {
    let noun = noun(side);
    let mut lines = vec![];
    if let Some(n) = stuck {
        lines.push(format!("沒有其他可用的{noun}裝置，暫時還在用 {n}。取消其他裝置的「不使用」並加入順序，或接上清單內的裝置。"));
    }
    if let Some(n) = paused {
        let until = match side {
            Side::Input => "裝置增減、設定改變，或麥克風有聲無聲翻轉",
            Side::Output => "裝置增減或設定改變",
        };
        lines.push(format!("其他程式或裝置一直把{noun}切回 {n}，Cleat 先暫停把它換掉，直到{until}才再試。想馬上再試，拔插一個裝置即可。"));
    }
    lines
}
pub const BLOCK_HELP: &str = "勾了之後，Cleat 永遠不會切到這個裝置";
pub const CPU_HELP: &str = "Cleat 常駐程式占一顆核心的百分比，跟活動監視器同一個算法；取最近 60 秒的平均，剛打開視窗時是打開以來的平均";
pub const MEMORY_HELP: &str = "與活動監視器「記憶體」欄同一個值";

/// 0.5 is "置中"; otherwise the side and |v - 0.5| x 200 percent.
pub fn balance_describe(value: f64) -> String {
    let percent = ((value - 0.5).abs() * 200.0).round() as i64;
    if percent == 0 {
        return "置中".into();
    }
    format!("{}{percent}%", if value < 0.5 { "偏左 " } else { "偏右 " })
}

/// "現在：AirPods Max 62%", or why there is no reading.
pub fn now_text(device: Option<&str>, reading: Option<&str>, noun: &str) -> String {
    match (device, reading) {
        (None, _) => format!("現在：沒有預設{noun}裝置"),
        (Some(d), None) => format!("現在：{d} 讀不到這個值"),
        (Some(d), Some(r)) => format!("現在：{d} {r}"),
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

pub fn vitals_headline(state: VitalsState) -> &'static str {
    match state {
        VitalsState::Running => "Cleat 執行中",
        VitalsState::NotRunning => "Cleat 未執行",
        VitalsState::Unreadable => "讀不到 Cleat 的用量",
    }
}

pub fn vitals_cpu(v: &DaemonVitals) -> String {
    match v.cpu_percent {
        Some(c) => cpu_text(c),
        None if v.state == VitalsState::Running && v.cpu_measuring => "量測中".into(),
        None => "—".into(),
    }
}

pub fn vitals_cpu_note(v: &DaemonVitals) -> String {
    v.cpu_window_seconds.map_or_else(|| "一顆核心的百分比".into(), |s| format!("最近 {s} 秒平均"))
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

pub fn reaction_note(v: &DaemonVitals) -> String {
    reaction(v).map_or_else(|| "還沒有拉回紀錄".into(), |p| format!("最近 {} 次的中位數", p.samples))
}

pub fn reaction_help(v: &DaemonVitals) -> String {
    if v.state != VitalsState::Running {
        return "Cleat 沒在執行".into();
    }
    let Some(p) = &v.performance else { return "這個版本的 Cleat 還不會量拉回速度".into() };
    let (Some(total), Some(work)) = (p.last_reaction_ms, p.last_work_ms) else { return "還沒有拉回紀錄".into() };
    if p.samples == 0 {
        return "還沒有拉回紀錄".into();
    }
    format!(
        "其他程式或系統改掉你的設定後，Cleat 改回來要多久。最近一次共 {}，其中 Cleat 自己處理 {}，其餘是刻意等裝置穩定",
        millis_text(total),
        millis_text(work)
    )
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
pub fn device_row_tags(r: &DeviceRow, current: Option<&str>, reason: RowReason) -> Vec<(&'static str, Tone)> {
    let why = match reason {
        RowReason::Stuck => ("已勾不使用，但沒有其他裝置可切", Tone::Warning),
        RowReason::Paused => ("已勾不使用，被切回來、暫停中", Tone::Warning),
        RowReason::None => ("已排除", Tone::Plain),
    };
    match (in_use(r, current), r.is_blocked()) {
        (true, true) => vec![("使用中", Tone::Accent), why],
        (true, false) => vec![("使用中", Tone::Accent)],
        (false, true) => vec![("已排除", Tone::Plain)],
        (false, false) => vec![],
    }
}

/// Only a "not used" device the side is not on is greyed out; the one in use stays readable.
pub fn row_dimmed(r: &DeviceRow, current: Option<&str>) -> bool {
    r.is_blocked() && !in_use(r, current)
}
