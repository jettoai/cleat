//! Every word the settings window shows, and the pure formatting behind them (Swift
//! `SettingsView.swift:22-48`, `LevelRows.swift`, `DaemonVitals.swift:70-168`, `SettingsIcon.swift`).

use objc2_core_audio::{
    kAudioDeviceTransportTypeAirPlay, kAudioDeviceTransportTypeBluetooth, kAudioDeviceTransportTypeBluetoothLE,
    kAudioDeviceTransportTypeBuiltIn, kAudioDeviceTransportTypeDisplayPort, kAudioDeviceTransportTypeHDMI,
};

use super::draft::{DeviceRow, Side};
use super::vitals::{DaemonVitals, VitalsState};
use crate::state::reaction_clock::DaemonPerformance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Output,
    Input,
    Headphones,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Output, Page::Input, Page::Headphones];

    pub fn title(self) -> &'static str {
        match self {
            Page::Output => "輸出",
            Page::Input => "輸入",
            Page::Headphones => "耳機",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Page::Output => "speaker.wave.2",
            Page::Input => "mic",
            Page::Headphones => "beats.headphones",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Output => "Cleat 讓聲音一直從你排第一的裝置出來",
            Page::Input => "Cleat 讓你排第一的麥克風一直是預設輸入，音量停在你設定的值",
            Page::Headphones => "藍牙耳機被手機或 iPad 拿走時，Cleat 把它要回來",
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
pub const OTHERS_NOTE: &str = "系統沒說是什麼的藍牙裝置。喇叭或耳機不在上面時，到這裡勾；手機、電腦不用勾。";
pub const HEADSET_BLOCKED_NOTE: &str = "輸出設為不使用，不會拉回";
/// The note under the row of the device a side is left on (B-1287); the page header says why.
pub const STUCK_ROW_NOTE: &str = "暫時還在用這台";
pub const ALL_HEADSETS_BLOCKED: &str = "勾選的耳機都設為不使用，不會有動作";

/// The page header's orange lines for one side (B-1287): left on a "not used" device because
/// nothing else is usable, and a "not used" device the eviction cooldown leaves in place. Each says
/// what Albert can do. The resume conditions follow `engine::eviction`'s log line.
pub fn stuck_header(side: Side, stuck: Option<&str>, paused: Option<&str>) -> Vec<String> {
    let noun = noun(side);
    let mut lines = vec![];
    if let Some(n) = stuck {
        lines.push(format!("沒有其他可用的{noun}裝置，暫時還在用 {n}。接上或連上清單裡的其他裝置，Cleat 就會切過去。"));
    }
    if let Some(n) = paused {
        let until = match side {
            Side::Input => "裝置增減、設定改變，或麥克風有聲無聲翻轉",
            Side::Output => "裝置增減或設定改變",
        };
        lines.push(format!("{n} 一直被切回來，Cleat 先暫停把它換掉，直到{until}才再試。想馬上再試，拔插一個裝置即可。"));
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

/// A device row's badge and whether it is the accent one. The device in use reads "使用中" even
/// when it is "not used" (B-1287: the side is left on it), so the row, the header and the level line
/// name the same device; any other "not used" device reads "已排除".
pub fn device_row_tag(r: &DeviceRow, current: Option<&str>) -> Option<(&'static str, bool)> {
    if r.is_connected && current.is_some_and(|c| crate::model::device_name::matches(&r.display_name, c, "")) {
        Some(("使用中", true))
    } else if r.is_blocked() {
        Some(("已排除", false))
    } else {
        None
    }
}
