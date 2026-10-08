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
