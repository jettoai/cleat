use cleat_rs::model::{TRANSPORT_BLUETOOTH, TRANSPORT_BUILT_IN, TRANSPORT_DISPLAY_PORT};
use cleat_rs::settings::draft::Side;
use cleat_rs::settings::sources::{last_revert, parse_paired};
use cleat_rs::settings::text::{balance_describe, byline, cpu_text, device_symbol, memory_text, now_text};

#[test]
fn balance_in_words() {
    let got: Vec<String> = [0.5, 0.49, 0.4, 0.0, 1.0].into_iter().map(balance_describe).collect();
    assert_eq!(got, ["置中", "偏左 2%", "偏左 20%", "偏左 100%", "偏右 100%"]);
}

#[test]
fn now_line_branches() {
    assert_eq!(now_text(None, None, "輸出"), "現在：沒有預設輸出裝置");
    assert_eq!(now_text(Some("Mic"), None, "輸入"), "現在：Mic 讀不到這個值");
    assert_eq!(now_text(Some("Mic"), Some("62%"), "輸入"), "現在：Mic 62%");
}

#[test]
fn vitals_formats() {
    assert_eq!([cpu_text(0.05), cpu_text(3.21), cpu_text(12.6)], ["0.05%", "3.2%", "13%"]);
    assert_eq!(memory_text(12_897_485), "12.3 MB");
}

#[test]
fn device_symbols() {
    assert_eq!(device_symbol("AirPods Max", None, Side::Output), "beats.headphones");
    assert_eq!(device_symbol("Speaker", Some(TRANSPORT_BLUETOOTH), Side::Output), "beats.headphones");
    assert_eq!(device_symbol("DELL U3223QE", Some(TRANSPORT_DISPLAY_PORT), Side::Output), "display");
    assert_eq!(device_symbol("DELL U3223QE", None, Side::Output), "hifispeaker");
    assert_eq!(device_symbol("MacBook Pro Speakers", Some(TRANSPORT_BUILT_IN), Side::Output), "laptopcomputer");
    assert_eq!(device_symbol("Brio 100", None, Side::Input), "web.camera");
    assert_eq!(device_symbol("Zoom", None, Side::Input), "mic");
}

#[test]
fn paired_devices_carry_their_class() {
    let json = br#"{"SPBluetoothDataType":[{"device_connected":[{"AirPods":{"device_address":"aa-bb","device_minorType":"Headphones"}}],"device_not_connected":[{"Box":{"device_address":"cc:dd"}}]}]}"#;
    let p = parse_paired(json);
    assert_eq!(p.len(), 2);
    assert_eq!((p[0].address.as_str(), p[0].minor_type.as_deref(), p[0].is_connected), ("AA:BB", Some("Headphones"), true));
    assert_eq!(p[1].minor_type, None);
}

#[test]
fn last_revert_is_optional() {
    assert_eq!(last_revert(br#"{"outputVolume":{"lastRevert":"13:40 x"}}"#).as_deref(), Some("13:40 x"));
    assert_eq!(last_revert(br#"{"pid":1}"#), None);
}

#[test]
fn byline_names_jetto() {
    assert_eq!(byline("0.1.0"), "Cleat 0.1.0 · by Jetto");
}

/// B-1287: the window reads all four `stuck` fields, shows them only while the daemon runs, and
/// the header gives each side its own lines with something to do.
#[test]
fn stuck_and_paused_reach_the_page_header_only_while_running() {
    use cleat_rs::settings::sources::stuck;
    use cleat_rs::settings::text::{stuck_header, STUCK_ROW_NOTE};
    use cleat_rs::settings::vitals::VitalsState;
    let data = br#"{"stuck": {"input": "AirPods Max", "output": null, "inputPaused": null, "outputPaused": "MacBook Pro Speakers"}}"#;
    let st = stuck(data);
    assert_eq!(st.side(Side::Input), (Some("AirPods Max"), None));
    assert_eq!(st.side(Side::Output), (None, Some("MacBook Pro Speakers")));
    assert_eq!(stuck(br#"{"stuck": {"input": "A", "output": "B"}}"#).side(Side::Output), (Some("B"), None), "an older file without the paused fields");
    assert_eq!(st.clone().shown_when(VitalsState::Running), st);
    assert_eq!(st.clone().shown_when(VitalsState::NotRunning), Default::default());
    assert_eq!(st.clone().shown_when(VitalsState::Unreadable), Default::default());

    assert!(stuck_header(Side::Input, None, None).is_empty());
    assert_eq!(
        stuck_header(Side::Input, Some("AirPods Max"), None),
        ["沒有其他可用的輸入裝置，暫時還在用 AirPods Max。取消其他裝置的「不使用」並加入順序，或接上清單內的裝置。"]
    );
    assert_eq!(
        stuck_header(Side::Output, None, Some("MacBook Pro Speakers")),
        ["其他程式或裝置一直把輸出切回 MacBook Pro Speakers，Cleat 先暫停把它換掉，直到裝置增減或設定改變才再試。想馬上再試，拔插一個裝置即可。"]
    );
    let input_paused = stuck_header(Side::Input, None, Some("AirPods Max"));
    assert!(input_paused[0].contains("麥克風有聲無聲翻轉"), "{input_paused:?}");
    let both = stuck_header(Side::Input, Some("A"), Some("B"));
    assert_eq!(both.len(), 2);
    assert!(both.iter().all(|l| !l.contains(STUCK_ROW_NOTE)), "the header and the device row say different things");
}

/// B-1287: the device a side is left on reads "使用中" even when it is "not used", so the row
/// agrees with the page header and the level line.
#[test]
fn the_device_in_use_reads_in_use_even_when_not_used() {
    use cleat_rs::settings::draft::{DeviceRow, Stance};
    use cleat_rs::settings::text::{device_row_tags as tags3, row_dimmed, RowReason, Tone};
    let device_row_tags = |r: &DeviceRow, c: Option<&str>| tags3(r, c, RowReason::Stuck);
    let row = |stance, is_connected| DeviceRow { entry: "Wireless microphone".into(), display_name: "Wireless microphone".into(), is_connected, stance };
    let now = Some("Wireless microphone");
    let other = Some("MacBook Pro Microphone");
    // In use and "not used": both badges, not greyed out (B-1287 round 2).
    assert_eq!(device_row_tags(&row(Stance::Blocked, true), now), [("使用中", Tone::Accent), ("已勾不使用，但沒有其他裝置可切", Tone::Warning)]);
    assert!(!row_dimmed(&row(Stance::Blocked, true), now));
    // In use, not blocked.
    assert_eq!(device_row_tags(&row(Stance::Listed, true), now), [("使用中", Tone::Accent)]);
    assert!(!row_dimmed(&row(Stance::Listed, true), now));
    // Not in use and "not used": greyed out.
    assert_eq!(device_row_tags(&row(Stance::Blocked, true), other), [("已排除", Tone::Plain)]);
    assert_eq!(device_row_tags(&row(Stance::Blocked, true), None), [("已排除", Tone::Plain)]);
    assert!(row_dimmed(&row(Stance::Blocked, true), other));
    assert!(device_row_tags(&row(Stance::Listed, true), other).is_empty());
    // Not connected: never in use, even when the name matches.
    assert_eq!(device_row_tags(&row(Stance::Blocked, false), now), [("已排除", Tone::Plain)]);
    assert!(row_dimmed(&row(Stance::Blocked, false), now));
    assert!(device_row_tags(&row(Stance::Listed, false), now).is_empty());
}

/// B-1287 round 2: the orange badge follows why the daemon left the side there; without a reason
/// (daemon not running, the instant before eviction) the in-use "not used" row keeps a grey "已排除".
#[test]
fn the_in_use_not_used_badge_follows_the_reason() {
    use cleat_rs::settings::draft::{DeviceRow, Stance};
    use cleat_rs::settings::text::{device_row_tags, RowReason, Tone};
    let r = DeviceRow { entry: "外接耳機".into(), display_name: "外接耳機".into(), is_connected: true, stance: Stance::Blocked };
    let now = Some("外接耳機");
    assert_eq!(device_row_tags(&r, now, RowReason::Stuck), [("使用中", Tone::Accent), ("已勾不使用，但沒有其他裝置可切", Tone::Warning)]);
    assert_eq!(device_row_tags(&r, now, RowReason::Paused), [("使用中", Tone::Accent), ("已勾不使用，被切回來、暫停中", Tone::Warning)]);
    assert_eq!(device_row_tags(&r, now, RowReason::None), [("使用中", Tone::Accent), ("已排除", Tone::Plain)]);
    assert_eq!(device_row_tags(&r, Some("DELL U3223QE"), RowReason::Paused), [("已排除", Tone::Plain)], "not in use: no reason badge");
}

/// B-1287 round 2: a blocked headset's note follows its tick; the four reasons have their words.
#[test]
fn headset_notes_and_reclaim_hints() {
    use cleat_rs::settings::draft::{HeadsetBox, ReclaimHint};
    use cleat_rs::settings::text::{headset_note, reclaim_hint_text, ALL_HEADSETS_BLOCKED};
    assert_eq!(headset_note(HeadsetBox::BlockedTicked, true), Some("輸出設為不使用，不會拉回"));
    assert_eq!(headset_note(HeadsetBox::BlockedOff, true), Some("輸出設為不使用"));
    assert_eq!(headset_note(HeadsetBox::Plain(true), false), Some("未連線"));
    assert_eq!(headset_note(HeadsetBox::Plain(false), true), None);
    assert_eq!(reclaim_hint_text(ReclaimHint::AllHeadsetsBlocked), "所有耳機都設為不使用，不會有動作。到「輸出」頁取消「不使用」即可。");
    assert_eq!(reclaim_hint_text(ReclaimHint::NoneTicked), "請至少勾選一副耳機，否則不會有動作");
    assert_eq!(reclaim_hint_text(ReclaimHint::AllTickedBlocked), ALL_HEADSETS_BLOCKED);
    assert_eq!(reclaim_hint_text(ReclaimHint::NoHeadsets), "還沒有配對過的耳機，不會有動作");
}
