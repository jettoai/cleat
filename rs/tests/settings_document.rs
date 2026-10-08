use cleat_rs::config::Config;
use cleat_rs::settings::document::{merged, write, Managed, WriteError};
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
fn rust_only_switches_and_unknown_keys_survive_a_write() {
    let raw = r#"{"reclaimEnabled":false,"outputVolumeHoldEnabled":true,"someUnknownKey":1}"#;
    let out = text(Some(raw), &default_managed());
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
    let r = write(&path, &default_managed(), Some(b"{\"output\":[\"A\"]}"));
    assert_eq!(r, Err(WriteError::ChangedOnDisk));
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"output\":[\"B\"]}");
    let ok = write(&path, &default_managed(), Some(b"{\"output\":[\"B\"]}")).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), ok);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn switching_a_feature_on_lifts_its_rust_only_false() {
    let raw = r#"{"reclaimEnabled":false,"outputVolumeHoldEnabled":false}"#;
    let mut m = default_managed();
    m.reclaim = vec!["AirPods Max".into()];
    m.hold_against = Some(vec!["Parallels Desktop".into()]);
    let out = text(Some(raw), &m);
    let config: Config = serde_json::from_str(&out).unwrap();
    assert_eq!(config.reclaim_active(), ["AirPods Max"], "{out}");
    assert_eq!(config.hold_against_active(), ["Parallels Desktop"], "{out}");
}

#[test]
fn switching_a_feature_on_adds_no_enabled_key() {
    let mut m = default_managed();
    m.reclaim = vec!["AirPods Max".into()];
    m.hold_against = Some(vec!["Parallels Desktop".into()]);
    let out = text(Some("{}"), &m);
    assert!(!out.contains("Enabled"), "{out}");
}
