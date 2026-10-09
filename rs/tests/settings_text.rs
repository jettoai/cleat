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
        ["沒有其他可用的輸入裝置，暫時還在用 AirPods Max。接上或連上清單裡的其他裝置，Cleat 就會切過去。"]
    );
    assert_eq!(
        stuck_header(Side::Output, None, Some("MacBook Pro Speakers")),
        ["MacBook Pro Speakers 一直被切回來，Cleat 先暫停把它換掉，直到裝置增減或設定改變才再試。想馬上再試，拔插一個裝置即可。"]
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
    use cleat_rs::settings::text::device_row_tag;
    let row = |stance| DeviceRow { entry: "Wireless microphone".into(), display_name: "Wireless microphone".into(), is_connected: true, stance };
    let now = Some("Wireless microphone");
    assert_eq!(device_row_tag(&row(Stance::Blocked), now), Some(("使用中", true)));
    assert_eq!(device_row_tag(&row(Stance::Blocked), Some("MacBook Pro Microphone")), Some(("已排除", false)));
    assert_eq!(device_row_tag(&row(Stance::Blocked), None), Some(("已排除", false)));
    assert_eq!(device_row_tag(&row(Stance::Listed), now), Some(("使用中", true)));
    assert_eq!(device_row_tag(&row(Stance::Listed), Some("MacBook Pro Microphone")), None);
}
