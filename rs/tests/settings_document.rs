use cleat_rs::config::Config;
use cleat_rs::settings::document::{merged, merged_from, write, Managed, Origin, WriteError};
use cleat_rs::settings::draft::{LiveLevels, SettingsDraft};

fn default_managed() -> Managed {
    SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default()).managed()
}

fn text(raw: Option<&str>, m: &Managed) -> String {
    String::from_utf8(merged(raw.map(str::as_bytes), m).unwrap()).unwrap()
}

#[test]
fn no_file_writes_keys_in_order_with_integer_volumes() {
    let mut m = default_managed();
    m.input_volume.insert("*".into(), 50.0);
    assert_eq!(
        text(None, &m),
        "{\n  \"input\": [],\n  \"blockedInput\": [],\n  \"output\": [],\n  \"blockedOutput\": [],\n  \"headphonesTakeOver\": false,\n  \"inputVolume\": {\"*\":50},\n  \"reclaim\": []\n}\n"
    );
}

#[test]
fn launch_at_login_on_writes_no_key_and_off_writes_false() {
    let on = text(None, &default_managed());
    assert!(!on.contains("launchAtLogin"), "{on}");
    let mut m = default_managed();
    m.launch_at_login = false;
    let off = text(None, &m);
    assert!(off.contains("  \"reclaim\": [],\n  \"launchAtLogin\": false\n"), "{off}");
}

#[test]
fn unknown_keys_and_liveness_are_carried_after_the_known_order() {
    let raw = r#"{"zeta":1,"liveness":{"Mic":{"zeroSeconds":5}},"alpha":"x","output":["A"]}"#;
    let out = text(Some(raw), &default_managed());
    let keys: Vec<&str> = out.lines().filter_map(|l| l.trim().split('"').nth(1)).collect();
    assert_eq!(keys, ["input", "blockedInput", "output", "blockedOutput", "headphonesTakeOver", "liveness", "reclaim", "alpha", "zeta"]);
    assert!(out.contains("  \"liveness\": {\"Mic\":{\"zeroSeconds\":5}},"));
}

#[test]
fn switches_off_remove_their_keys() {
    let raw = r#"{"balance":0.4,"outputVolumeHoldAgainst":["X"]}"#;
    let out = text(Some(raw), &default_managed());
    assert!(!out.contains("balance") && !out.contains("outputVolumeHoldAgainst"), "{out}");
}

#[test]
fn rust_only_switches_and_unknown_keys_survive_an_untouched_save() {
    let raw = r#"{"reclaimEnabled":false,"outputVolumeHoldEnabled":true,"someUnknownKey":1}"#;
    let config: Config = serde_json::from_str(raw).unwrap();
    let m = SettingsDraft::make(&config, &[], &[], &LiveLevels::default()).managed();
    let origin = Origin::new(Some(raw.as_bytes()), m.clone());
    let out = String::from_utf8(merged_from(Some(raw.as_bytes()), &m, Some(&origin)).unwrap()).unwrap();
    assert!(out.contains("  \"reclaimEnabled\": false"), "{out}");
    assert!(out.contains("  \"outputVolumeHoldEnabled\": true"), "{out}");
    assert!(out.contains("  \"someUnknownKey\": 1"), "{out}");
}

#[test]
fn numbers_keep_their_shortest_form() {
    let mut m = default_managed();
    m.balance = Some(0.45);
    m.input_volume.insert("Mic".into(), 50.0);
    let out = text(None, &m);
    assert!(out.contains("  \"balance\": 0.45,"), "{out}");
    assert!(out.contains("{\"Mic\":50}"), "{out}");
}

#[test]
fn a_non_object_file_is_refused() {
    assert_eq!(merged(Some(b"[1]"), &default_managed()), Err(WriteError::NotAnObject));
}

#[test]
fn an_impossible_value_is_refused() {
    let mut m = default_managed();
    m.balance = Some(1.5);
    assert!(matches!(merged(None, &m), Err(WriteError::Invalid(_))));
}

#[test]
fn names_are_written_verbatim() {
    let mut m = default_managed();
    m.output = vec!["Mac\u{00A0}Studio/Speakers".into()];
    let out = text(None, &m);
    assert!(out.contains("\"Mac\u{00A0}Studio/Speakers\""), "{out}");
}

#[test]
fn a_file_changed_on_disk_is_not_overwritten() {
    let dir = std::env::temp_dir().join(format!("cleat-settings-doc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, b"{\"output\":[\"B\"]}").unwrap();
    let r = write(&path, &default_managed(), None, Some(b"{\"output\":[\"A\"]}"));
    assert_eq!(r, Err(WriteError::ChangedOnDisk));
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"output\":[\"B\"]}");
    let ok = write(&path, &default_managed(), None, Some(b"{\"output\":[\"B\"]}")).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), ok);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn switching_a_feature_on_lifts_its_rust_only_false() {
    let raw = r#"{"reclaimEnabled":false,"outputVolumeHoldEnabled":false}"#;
    let mut m = default_managed();
    m.reclaim = vec!["AirPods Max".into()];
    m.reclaim_on = true;
    m.hold_against = vec!["Parallels Desktop".into()];
    m.hold_on = true;
    let out = text(Some(raw), &m);
    let config: Config = serde_json::from_str(&out).unwrap();
    assert_eq!(config.reclaim_active(), ["AirPods Max"], "{out}");
    assert_eq!(config.hold_against_active(), ["Parallels Desktop"], "{out}");
}

#[test]
fn switching_a_feature_on_adds_no_enabled_key() {
    let mut m = default_managed();
    m.reclaim = vec!["AirPods Max".into()];
    m.reclaim_on = true;
    m.hold_against = vec!["Parallels Desktop".into()];
    m.hold_on = true;
    let out = text(Some("{}"), &m);
    assert!(!out.contains("Enabled"), "{out}");
}

fn save_untouched(raw: &str, touch: impl FnOnce(&mut SettingsDraft)) -> serde_json::Value {
    let config: Config = serde_json::from_str(raw).unwrap();
    let mut draft = SettingsDraft::make(&config, &[], &[], &LiveLevels::default());
    let origin = Origin::new(Some(raw.as_bytes()), draft.managed());
    touch(&mut draft);
    let out = merged_from(Some(raw.as_bytes()), &draft.managed(), Some(&origin)).unwrap();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn a_reclaim_list_parked_behind_false_survives_an_unrelated_save() {
    let raw = r#"{"reclaim":["AirPods Max"],"reclaimEnabled":false}"#;
    let out = save_untouched(raw, |d| d.headphones_take_over = !d.headphones_take_over);
    assert_eq!(out["reclaim"], serde_json::json!(["AirPods Max"]), "{out}");
    assert_eq!(out["reclaimEnabled"], serde_json::json!(false), "{out}");
}

#[test]
fn a_hold_list_parked_behind_false_survives_an_unrelated_save() {
    let raw = r#"{"outputVolumeHoldAgainst":["Zoom","Parallels Desktop"],"outputVolumeHoldEnabled":false}"#;
    let out = save_untouched(raw, |d| d.headphones_take_over = !d.headphones_take_over);
    assert_eq!(out["outputVolumeHoldAgainst"], serde_json::json!(["Zoom", "Parallels Desktop"]), "{out}");
    assert_eq!(out["outputVolumeHoldEnabled"], serde_json::json!(false), "{out}");
}

#[test]
fn touching_a_parked_switch_writes_what_the_window_shows() {
    let raw = r#"{"reclaim":["AirPods Max"],"reclaimEnabled":false,"outputVolumeHoldAgainst":["Zoom"],"outputVolumeHoldEnabled":false}"#;
    let on = save_untouched(raw, |d| {
        d.set_reclaim_enabled(true);
        d.set_hold_enabled(true);
    });
    assert_eq!(on["reclaim"], serde_json::json!(["AirPods Max"]), "{on}");
    assert_eq!(on["outputVolumeHoldAgainst"], serde_json::json!(["Zoom"]), "{on}");
    assert!(on.get("reclaimEnabled").is_none() && on.get("outputVolumeHoldEnabled").is_none(), "{on}");
    let off = save_untouched(raw, |d| {
        d.set_reclaim_enabled(false);
        d.set_hold_enabled(false);
    });
    assert_eq!(off["reclaim"], serde_json::json!(["AirPods Max"]), "{off}");
    assert_eq!(off["reclaimEnabled"], serde_json::json!(false), "{off}");
    assert_eq!(off["outputVolumeHoldAgainst"], serde_json::json!(["Zoom"]), "{off}");
    assert_eq!(off["outputVolumeHoldEnabled"], serde_json::json!(false), "{off}");
}

#[test]
fn usage_ping_survives_a_save_and_sits_after_error_reports() {
    let raw = r#"{"usagePing":false,"errorReports":true,"reclaim":[]}"#;
    let config: Config = serde_json::from_str(raw).unwrap();
    let m = SettingsDraft::make(&config, &[], &[], &LiveLevels::default()).managed();
    let origin = Origin::new(Some(raw.as_bytes()), m.clone());
    let out = String::from_utf8(merged_from(Some(raw.as_bytes()), &m, Some(&origin)).unwrap()).unwrap();
    let keys: Vec<&str> = out.lines().filter_map(|l| l.trim().split('"').nth(1)).collect();
    assert_eq!(keys.iter().rev().take(2).copied().collect::<Vec<_>>(), ["usagePing", "errorReports"]);
    assert!(out.contains("\"usagePing\": false"), "{out}");
}

#[test]
fn heic_keys_survive_a_save_even_when_hand_edited() {
    let raw = r#"{"convertHeic":true,"heic":{"watch":"~/Pictures/in","originals":"/Volumes/x/orig","longEdge":3200,"quality":70,"extra":[1]},"reclaim":[]}"#;
    let original: serde_json::Value = serde_json::from_str(raw).unwrap();
    let out = save_untouched(raw, |d| d.headphones_take_over = !d.headphones_take_over);
    assert_eq!(out["convertHeic"], original["convertHeic"], "{out}");
    assert_eq!(out["heic"], original["heic"], "{out}");
    let config: Config = serde_json::from_value(out).unwrap();
    assert!(config.convert_heic);
    assert_eq!((config.heic.long_edge, config.heic.quality), (3200, 70));
}

#[test]
fn heic_keys_are_not_added_to_a_file_that_has_none() {
    let out = text(Some(r#"{"reclaim":[]}"#), &default_managed());
    assert!(!out.contains("heic") && !out.contains("Heic"), "{out}");
}

/// The bytes a save of `raw` writes after `touch`, with the window's origin as at load.
fn saved_bytes(raw: &str, touch: impl FnOnce(&mut SettingsDraft)) -> String {
    let config: Config = serde_json::from_str(raw).unwrap();
    let mut draft = SettingsDraft::make(&config, &[], &[], &LiveLevels::default());
    let origin = Origin::new(Some(raw.as_bytes()), draft.managed());
    touch(&mut draft);
    String::from_utf8(merged_from(Some(raw.as_bytes()), &draft.managed(), Some(&origin)).unwrap()).unwrap()
}

const HAND_EDITED_HEIC: &str =
    r#"{"convertHeic":true,"heic":{"watch":"~/Pictures/in","originals":"/Volumes/x/orig","longEdge":3200,"quality":70,"extra":[1]},"reclaim":[]}"#;

#[test]
fn the_photos_page_edits_reach_the_file_and_leave_quality_and_unknown_keys() {
    let home = std::path::Path::new("/Users/me");
    let out: serde_json::Value = serde_json::from_str(&saved_bytes(HAND_EDITED_HEIC, |d| {
        d.convert_heic = false;
        d.heic_long_edge = 1600;
        d.set_heic_watch(std::path::Path::new("/Users/me/Downloads/in"), home);
        d.set_heic_originals(std::path::Path::new("/Users/me/Downloads/in/Raw"), home);
    }))
    .unwrap();
    assert!(out.get("convertHeic").is_none(), "{out}");
    assert_eq!(
        out["heic"],
        serde_json::json!({"watch": "~/Downloads/in", "originals": "Raw", "longEdge": 1600, "quality": 70, "extra": [1]}),
        "{out}"
    );
    let on: serde_json::Value = serde_json::from_str(&saved_bytes(r#"{"reclaim":[]}"#, |d| d.convert_heic = true)).unwrap();
    assert_eq!(on["convertHeic"], serde_json::json!(true), "{on}");
    assert!(on.get("heic").is_none(), "switching on alone writes no heic cells: {on}");
}

#[test]
fn a_save_with_no_photos_edit_keeps_the_bytes() {
    for raw in [HAND_EDITED_HEIC, r#"{"reclaim":[]}"#, r#"{"heic":{},"reclaim":[]}"#, r#"{"convertHeic":false,"heic":{"quality":60},"reclaim":[]}"#] {
        let canonical = saved_bytes(raw, |_| {});
        assert_eq!(saved_bytes(&canonical, |_| {}), canonical, "{raw}");
        let unchanged: serde_json::Value = serde_json::from_str(raw).unwrap();
        let written: serde_json::Value = serde_json::from_str(&canonical).unwrap();
        assert_eq!(written, unchanged, "{raw}");
    }
}

#[test]
fn only_the_edited_cell_is_written_and_editing_back_restores_the_file() {
    let raw = r#"{"heic":{"quality":70},"reclaim":[]}"#;
    let out: serde_json::Value = serde_json::from_str(&saved_bytes(raw, |d| d.heic_long_edge = 2400)).unwrap();
    assert_eq!(out["heic"], serde_json::json!({"quality": 70, "longEdge": 2400}), "{out}");

    // Away and back, saved once: the file as it was, not a cell spelling out the default.
    let back: serde_json::Value = serde_json::from_str(&saved_bytes(raw, |d| {
        d.heic_long_edge = 2400;
        d.heic_long_edge = 1200;
    }))
    .unwrap();
    assert_eq!(back["heic"], serde_json::json!({"quality": 70}), "{back}");
    let none: serde_json::Value = serde_json::from_str(&saved_bytes(r#"{"reclaim":[]}"#, |d| d.heic_watch = "~/x".into())).unwrap();
    assert_eq!(none["heic"], serde_json::json!({"watch": "~/x"}), "{none}");
}

/// Without an origin (the pure `merged`), a cell is written only when the file does not say it.
#[test]
fn without_an_origin_a_cell_is_written_when_the_file_differs() {
    let mut m = default_managed();
    m.heic_long_edge = 1200;
    let same = text(Some(r#"{"heic":{"longEdge":1200,"quality":70}}"#), &m);
    assert!(same.contains("\"longEdge\":1200") && same.contains("\"quality\":70"), "{same}");
    let differs = text(Some(r#"{"heic":{"longEdge":2400,"quality":70}}"#), &m);
    assert!(differs.contains("\"longEdge\":1200") && differs.contains("\"quality\":70"), "{differs}");
    let default_in_file = text(Some(r#"{"heic":{"quality":70}}"#), &m);
    assert!(!default_in_file.contains("longEdge"), "{default_in_file}");
}

#[test]
fn an_originals_folder_inside_the_watch_folder_is_kept_as_its_name() {
    let home = std::path::Path::new("/Users/me");
    let mut d = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    d.set_heic_originals(std::path::Path::new("/Users/me/Downloads/HEIC原檔"), home);
    assert_eq!(d.heic_originals, "HEIC原檔");
    d.set_heic_originals(std::path::Path::new("/Users/me/Pictures/Raw"), home);
    assert_eq!(d.heic_originals, "~/Pictures/Raw");
    d.set_heic_originals(std::path::Path::new("/Volumes/x/orig"), home);
    assert_eq!(d.heic_originals, "/Volumes/x/orig");
    d.set_heic_watch(std::path::Path::new("/Users/me"), home);
    assert_eq!(d.heic_watch, "~");
}

#[test]
fn the_photos_page_never_makes_the_originals_folder_the_watched_one() {
    let home = std::path::Path::new("/Users/me");
    let mut d = SettingsDraft::make(&Config::disabled(), &[], &[], &LiveLevels::default());
    d.set_heic_watch(std::path::Path::new("/Users/me/Pictures/in"), home);
    d.set_heic_originals(std::path::Path::new("/Users/me/Pictures/in"), home);
    assert_eq!(d.heic_originals, "HEIC原檔", "the watched folder is refused as originals");
    d.set_heic_originals(std::path::Path::new("/Volumes/x/orig"), home);
    d.set_heic_watch(std::path::Path::new("/Volumes/x/orig/"), home);
    assert_eq!(d.heic_watch, "~/Pictures/in", "the folder the originals go to is refused as watch");
    d.set_heic_originals(std::path::Path::new("/Users/me/Pictures/in/Raw"), home);
    assert_eq!(d.heic_originals, "Raw", "a folder inside the watched one is still kept as its name");
}
