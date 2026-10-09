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
