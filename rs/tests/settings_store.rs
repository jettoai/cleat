//! The window's save path end to end: CLEAT_CONFIG names the file, an edit is written into it, keys
//! the window does not own stay, and a second save is not mistaken for someone else's write.
//! One test per binary: it sets a process-wide environment variable.

use cleat_rs::settings::store::{Phase, Store};

#[test]
fn saves_go_to_the_cleat_config_file_and_keep_what_the_window_does_not_own() {
    let dir = std::env::temp_dir().join(format!("cleat-rs-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let original = r#"{"launchAtLogin": false, "zeta": 1, "blockedInput": ["AirPods Max"]}"#;
    std::fs::write(&path, original).unwrap();
    std::env::set_var("CLEAT_CONFIG", &path);

    let mut store = Store::new();
    assert!(store.begin_load());
    store.apply_paired(&[]);
    assert_eq!(store.phase, Phase::Ready);

    store.draft.balance_enabled = true;
    store.save_now();
    assert_eq!(store.error_message, None);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["balance"], serde_json::json!(store.draft.balance));
    assert_eq!(saved["zeta"], 1);
    assert_eq!(saved["blockedInput"], serde_json::json!(["AirPods\u{00A0}Max"]), "untouched unit kept verbatim");

    store.draft.balance_enabled = false;
    store.save_now();
    assert_eq!(store.error_message, None);
    assert!(!store.has_conflict);
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(saved.get("balance").is_none(), "{saved}");
    assert_eq!(saved["blockedInput"], serde_json::json!(["AirPods\u{00A0}Max"]));
    let _ = std::fs::remove_dir_all(&dir);
}
